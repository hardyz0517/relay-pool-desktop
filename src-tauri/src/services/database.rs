use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, params_from_iter, types::Type, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::models::{
    change_events::{ChangeEvent, UpsertChangeEventInput},
    channel_monitors::{
        ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
        CreateChannelMonitorInput, CreateChannelMonitorRunInput, CreateChannelMonitorTemplateInput,
        UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
    },
    collector::CollectorSnapshot,
    collector_runs::{CollectorRun, CreateCollectorRunInput, FinishCollectorRunInput},
    credentials::{
        PersistStationSessionInput, StationCredentials, StationSessionCredentialKind,
        UpdateStationCredentialsInput, UpdateStationSessionInput,
    },
    group_facts::{
        GroupRateRecord, InsertGroupRateRecordInput, StationGroupBinding,
        UpdateStationKeyGroupBindingInput, UpsertStationGroupBindingInput,
    },
    pricing::{
        BalanceSnapshot, ModelBasePrice, PricingRule, UpsertBalanceSnapshotInput,
        UpsertModelBasePriceInput, UpsertPricingRuleInput,
    },
    proxy::{CreateRequestLogInput, RequestLog, UpstreamApiFormat},
    remote_keys::{RemoteKeyMatchStatus, RemoteStationKey},
    routing::{
        ModelAlias, PricingGroupType, RouteSimulationInput, RouteSimulationResult,
        RoutingGroupFilter, RoutingPolicy, SchedulerAdvancedSettings, StationKeyCapabilities,
        StationKeyHealth, UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput,
    },
    secrets::{SecretMigrationReport, SecretScanFinding},
    settings::{AppSettings, UpdateSettingsInput},
    shared_capabilities::{
        ChannelMonitorSummary, ChannelStatusSummary, ChannelStatusTimelinePoint,
        SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult, StationGroupOption,
    },
    station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
    stations::{CreateStationInput, Station, StationEndpointHealth, UpdateStationInput},
};

const CHANNEL_STATUS_TIMELINE_LIMIT: usize = 60;
const INSECURE_LOCAL_KEY_PLACEHOLDER: &str = "sk-local-pool-change-me";

#[derive(Debug, Clone)]
pub struct ChannelStatusWindowFacts {
    pub total_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub warning_count: i64,
    pub avg_latency_ms: Option<i64>,
    pub avg_endpoint_ping_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub latest_status: Option<String>,
    pub latest_error_message: Option<String>,
    pub timeline: Vec<ChannelStatusTimelinePoint>,
}
use crate::services::change_events::{
    STATUS_DISMISSED, STATUS_READ, STATUS_RESOLVED, STATUS_UNREAD,
};
use crate::services::collectors::session::{token_is_fresh, ResolvedSession, SessionResolveStatus};
use crate::services::group_categories::normalize_group_category;
use crate::services::outbound::{normalize_proxy_mode, normalize_proxy_url, resolve_proxy_config};
use crate::services::pricing::sanitize_pricing_rule_input;
use crate::services::proxy::{
    router::{select_route_candidates, RichRouteCandidate, RouteCandidateEconomics, RouteRequest},
    routing_snapshot::LocalRoutingReadCandidate,
    scheduler::{
        affinity::AffinityStore,
        capacity::CapacityRegistry,
        metrics::RuntimeMetricsRegistry,
        multiplier::resolve_effective_multiplier,
        schedule_once,
        types::{MultiplierSourceFacts, SchedulerCandidate},
    },
    RouteCandidate,
};
use crate::services::secrets::{
    crypto::{decrypt_secret, encrypt_secret, EncryptedPayload},
    mask::{
        mask_secret as mask_sensitive_value, redact_text as redact_sensitive_text,
        redact_value as redact_sensitive_value,
    },
};
use crate::services::station_endpoints::{
    legacy_api_base_url, legacy_website_url, normalize_station_endpoints, same_origin,
};

static NEXT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
const DEFAULT_SETTINGS: [(&str, &str); 18] = [
    ("local_proxy_port", "8787"),
    ("local_key", "sk-local-pool-change-me"),
    ("default_routing_strategy", "cost_stable_first"),
    ("collector_proxy_mode", "direct"),
    ("collector_proxy_url", ""),
    ("max_rate_multiplier", ""),
    ("default_routing_group_filter", "all_groups"),
    ("scheduler_advanced_settings_json", ""),
    ("low_balance_threshold_cny", "15"),
    ("collector_interval_minutes", "30"),
    ("balance_interval_minutes", "5"),
    ("group_rate_interval_minutes", "20"),
    ("model_list_interval_minutes", "60"),
    ("pricing_refresh_interval_minutes", "60"),
    ("collector_timeout_seconds", "15"),
    ("collector_max_concurrency", "3"),
    ("allow_depleted_fallback", "false"),
    ("developer_mode_enabled", "false"),
];

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataDirConfig {
    pending_data_dir: Option<String>,
    source_data_dir: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct DataDirConfigPaths {
    pending_data_dir: Option<PathBuf>,
    source_data_dir: Option<PathBuf>,
}
#[derive(Clone)]
pub struct AppDatabase {
    connection: Arc<Mutex<Connection>>,
    data_dir: PathBuf,
    db_path: PathBuf,
    default_data_dir: PathBuf,
    data_dir_config_path: PathBuf,
    pending_data_dir: Arc<Mutex<Option<PathBuf>>>,
}

fn read_data_dir_config(config_path: &PathBuf) -> Result<DataDirConfigPaths, String> {
    if !config_path.exists() {
        return Ok(DataDirConfigPaths::default());
    }
    let raw = fs::read_to_string(config_path)
        .map_err(|error| format!("读取数据目录配置 {} 失败: {error}", config_path.display()))?;
    let config = serde_json::from_str::<DataDirConfig>(&raw)
        .map_err(|error| format!("解析数据目录配置 {} 失败: {error}", config_path.display()))?;
    Ok(DataDirConfigPaths {
        pending_data_dir: config
            .pending_data_dir
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty()),
        source_data_dir: config
            .source_data_dir
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty()),
    })
}

fn write_data_dir_config(
    config_path: &PathBuf,
    data_dir: &PathBuf,
    source_data_dir: Option<&Path>,
) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建数据目录配置目录 {}: {error}", parent.display()))?;
    }
    let config = DataDirConfig {
        pending_data_dir: Some(data_dir.to_string_lossy().to_string()),
        source_data_dir: source_data_dir.map(|path| path.to_string_lossy().to_string()),
    };
    let raw = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("序列化数据目录配置失败: {error}"))?;
    fs::write(config_path, raw)
        .map_err(|error| format!("写入数据目录配置 {} 失败: {error}", config_path.display()))
}

fn mark_data_dir_initialized(config_path: &PathBuf, data_dir: &Path) -> Result<(), String> {
    write_data_dir_config(config_path, &data_dir.to_path_buf(), Some(data_dir))
}

fn resolve_configured_data_dir(
    default_data_dir: &Path,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>), String> {
    let data_dir_config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
    let config = read_data_dir_config(&data_dir_config_path)?;
    let configured_data_dir = config
        .pending_data_dir
        .clone()
        .unwrap_or_else(|| default_data_dir.to_path_buf());
    Ok((
        configured_data_dir,
        config.pending_data_dir,
        config.source_data_dir,
    ))
}

fn prepare_configured_database(
    default_data_dir: &Path,
    configured_data_dir: &Path,
    source_data_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(configured_data_dir).map_err(|error| {
        format!(
            "无法创建应用数据目录 {}: {error}",
            configured_data_dir.display()
        )
    })?;

    let db_path = configured_data_dir.join(DATABASE_FILE);
    let source_data_dir = source_data_dir.unwrap_or(default_data_dir);
    if configured_data_dir != source_data_dir
        && should_copy_source_database(source_data_dir, &db_path)?
    {
        let source_db_path = source_data_dir.join(DATABASE_FILE);
        fs::copy(&source_db_path, &db_path).map_err(|error| {
            format!(
                "无法将现有数据库 {} 复制到新数据目录 {}: {error}",
                source_db_path.display(),
                db_path.display()
            )
        })?;
    }

    Ok(db_path)
}

fn should_copy_source_database(
    source_data_dir: &Path,
    target_db_path: &Path,
) -> Result<bool, String> {
    let source_db_path = source_data_dir.join(DATABASE_FILE);
    let Some(source_state) = inspect_sqlite_user_state(&source_db_path)? else {
        return Ok(false);
    };
    if !source_state.has_user_state() {
        return Ok(false);
    }

    let target_has_user_state = inspect_sqlite_user_state(target_db_path)?
        .map(|state| state.has_user_state())
        .unwrap_or(false);
    Ok(!target_has_user_state)
}

struct SqliteUserState {
    station_count: i64,
    has_custom_settings: bool,
}

impl SqliteUserState {
    fn has_user_state(&self) -> bool {
        self.station_count > 0 || self.has_custom_settings
    }
}

fn inspect_sqlite_user_state(db_path: &Path) -> Result<Option<SqliteUserState>, String> {
    if !db_path.exists() {
        return Ok(None);
    }

    let connection = Connection::open(db_path)
        .map_err(|error| format!("无法检查 SQLite 数据库 {}: {error}", db_path.display()))?;
    let has_stations_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'stations'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("无法检查 SQLite 表结构 {}: {error}", db_path.display()))?;
    if has_stations_table == 0 {
        return Ok(Some(SqliteUserState {
            station_count: 0,
            has_custom_settings: false,
        }));
    }

    let station_count = connection
        .query_row("SELECT COUNT(*) FROM stations", [], |row| row.get(0))
        .map_err(|error| format!("无法检查站点数量 {}: {error}", db_path.display()))?;
    let has_settings_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| {
            format!(
                "Cannot inspect SQLite schema {}: {error}",
                db_path.display()
            )
        })?;
    let has_custom_settings = if has_settings_table == 0 {
        false
    } else {
        sqlite_has_custom_settings(&connection, db_path)?
    };

    Ok(Some(SqliteUserState {
        station_count,
        has_custom_settings,
    }))
}

fn sqlite_has_custom_settings(connection: &Connection, db_path: &Path) -> Result<bool, String> {
    let mut statement = connection
        .prepare("SELECT key, value FROM settings")
        .map_err(|error| format!("Cannot inspect settings {}: {error}", db_path.display()))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("Cannot inspect settings {}: {error}", db_path.display()))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("Cannot inspect settings {}: {error}", db_path.display()))?
    {
        let key: String = row.get(0).map_err(|error| {
            format!("Cannot inspect setting key {}: {error}", db_path.display())
        })?;
        let value: String = row.get(1).map_err(|error| {
            format!(
                "Cannot inspect setting value {}: {error}",
                db_path.display()
            )
        })?;
        if !matches!(default_setting_value(&key), Some(default_value) if default_value == value) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn default_setting_value(key: &str) -> Option<&'static str> {
    DEFAULT_SETTINGS
        .iter()
        .find_map(|(setting_key, value)| (*setting_key == key).then_some(*value))
}

impl AppDatabase {
    pub fn initialize(app: &AppHandle) -> Result<Self, String> {
        let default_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法解析应用数据目录: {error}"))?;
        let data_dir_config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
        let (configured_data_dir, pending_data_dir, source_data_dir) =
            resolve_configured_data_dir(&default_data_dir)?;

        fs::create_dir_all(&configured_data_dir).map_err(|error| {
            format!(
                "无法创建应用数据目录 {}: {error}",
                configured_data_dir.display()
            )
        })?;

        let db_path = prepare_configured_database(
            &default_data_dir,
            &configured_data_dir,
            source_data_dir.as_deref(),
        )?;
        if pending_data_dir.is_some()
            && source_data_dir.as_deref() != Some(configured_data_dir.as_path())
        {
            mark_data_dir_initialized(&data_dir_config_path, &configured_data_dir)?;
        }
        let connection = Connection::open(&db_path)
            .map_err(|error| format!("无法打开 SQLite 数据库 {}: {error}", db_path.display()))?;

        initialize_schema(&connection)
            .map_err(|error| format!("初始化 SQLite schema 失败: {error}"))?;
        migrate_station_endpoint_urls(&connection)
            .map_err(|error| format!("migrate station endpoint URLs failed: {error}"))?;
        migrate_secret_schema(&connection)
            .map_err(|error| format!("迁移凭据安全字段失败: {error}"))?;
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_routing_strategy(&connection)
            .map_err(|error| format!("migrate default routing strategy failed: {error}"))?;
        migrate_automatic_scheduler_schema(&connection)
            .map_err(|error| format!("migrate automatic scheduler schema failed: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_legacy_group_facts(&connection)
            .map_err(|error| format!("迁移旧分组事实失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_pricing_tables(&connection)
            .map_err(|error| format!("迁移价格和余额表失败: {error}"))?;
        seed_builtin_model_base_prices(&connection)
            .map_err(|error| format!("初始化模型基准价格失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;
        migrate_request_log_cost_columns(&connection)
            .map_err(|error| format!("迁移请求日志成本字段失败: {error}"))?;
        migrate_request_log_economic_columns(&connection)
            .map_err(|error| format!("迁移请求日志经济上下文字段失败: {error}"))?;
        migrate_request_log_lifecycle_columns(&connection)
            .map_err(|error| format!("迁移请求日志生命周期字段失败: {error}"))?;
        migrate_request_log_observability_columns(&connection)
            .map_err(|error| format!("迁移请求日志观测字段失败: {error}"))?;
        migrate_remote_key_tables(&connection)
            .map_err(|error| format!("迁移远端 Key 表失败: {error}"))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            data_dir: configured_data_dir,
            db_path,
            default_data_dir,
            data_dir_config_path,
            pending_data_dir: Arc::new(Mutex::new(pending_data_dir)),
        })
    }

    #[cfg(test)]
    pub fn new_in_memory_for_tests() -> Result<Self, String> {
        let connection = Connection::open_in_memory()
            .map_err(|error| format!("无法打开内存 SQLite 数据库: {error}"))?;
        initialize_schema(&connection)
            .map_err(|error| format!("初始化 SQLite schema 失败: {error}"))?;
        migrate_station_endpoint_urls(&connection)
            .map_err(|error| format!("migrate station endpoint URLs failed: {error}"))?;
        migrate_secret_schema(&connection)
            .map_err(|error| format!("迁移凭据安全字段失败: {error}"))?;
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_routing_strategy(&connection)
            .map_err(|error| format!("migrate default routing strategy failed: {error}"))?;
        migrate_automatic_scheduler_schema(&connection)
            .map_err(|error| format!("migrate automatic scheduler schema failed: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_legacy_group_facts(&connection)
            .map_err(|error| format!("迁移旧分组事实失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_pricing_tables(&connection)
            .map_err(|error| format!("迁移价格和余额表失败: {error}"))?;
        seed_builtin_model_base_prices(&connection)
            .map_err(|error| format!("初始化模型基准价格失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;
        migrate_request_log_cost_columns(&connection)
            .map_err(|error| format!("迁移请求日志成本字段失败: {error}"))?;
        migrate_request_log_economic_columns(&connection)
            .map_err(|error| format!("迁移请求日志经济上下文字段失败: {error}"))?;
        migrate_request_log_lifecycle_columns(&connection)
            .map_err(|error| format!("迁移请求日志生命周期字段失败: {error}"))?;
        migrate_request_log_observability_columns(&connection)
            .map_err(|error| format!("迁移请求日志观测字段失败: {error}"))?;
        migrate_remote_key_tables(&connection)
            .map_err(|error| format!("迁移远端 Key 表失败: {error}"))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            data_dir: PathBuf::from(":memory:"),
            db_path: PathBuf::from(":memory:"),
            default_data_dir: PathBuf::from(":memory:"),
            data_dir_config_path: PathBuf::from(":memory:"),
            pending_data_dir: Arc::new(Mutex::new(None)),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "SQLite 连接锁已损坏".to_string())
    }

    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    pub fn migrate_plaintext_secrets(
        &self,
        data_key: &[u8; 32],
    ) -> Result<SecretMigrationReport, String> {
        let connection = self.connection()?;
        migrate_plaintext_secrets_in_connection(&connection, data_key)
    }

    pub fn resolve_station_key_secret_with_data_key(
        &self,
        data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String> {
        let connection = self.connection()?;
        resolve_station_key_api_key(&connection, data_key, station_key_id)
    }

    pub fn secret_migration_status(&self) -> Result<SecretMigrationReport, String> {
        let connection = self.connection()?;
        secret_migration_status_from_connection(&connection)
    }

    pub fn run_secret_safety_scan(&self) -> Result<Vec<SecretScanFinding>, String> {
        let connection = self.connection()?;
        run_secret_safety_scan_in_connection(&connection)
    }

    #[cfg(test)]
    pub fn migrate_plaintext_secrets_for_tests(
        &self,
        data_key: &[u8; 32],
    ) -> Result<SecretMigrationReport, String> {
        self.migrate_plaintext_secrets(data_key)
    }

    #[cfg(test)]
    pub fn proxy_route_candidates_with_data_key_for_tests(
        &self,
        data_key: &[u8; 32],
    ) -> Result<Vec<RouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_route_candidates_from_connection_with_data_key(&connection, Some(data_key))
    }

    #[cfg(test)]
    pub fn resolve_station_key_secret_for_tests(
        &self,
        data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String> {
        let connection = self.connection()?;
        resolve_station_key_api_key(&connection, data_key, station_key_id)
    }

    #[cfg(test)]
    pub fn clear_station_key_secret_for_tests(&self, station_key_id: &str) -> Result<(), String> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE station_keys
                    SET api_key = '',
                        api_key_secret_id = NULL,
                        updated_at = ?1
                  WHERE id = ?2",
                params![now_string(), station_key_id],
            )
            .map_err(|error| format!("Clear station key secret failed: {error}"))?;
        Ok(())
    }

    pub fn list_stations(&self) -> Result<Vec<Station>, String> {
        let connection = self.connection()?;
        list_stations_from_connection(&connection)
    }

    pub fn create_station(&self, input: CreateStationInput) -> Result<Station, String> {
        self.create_station_with_data_key(input, None)
    }

    pub fn create_station_with_data_key(
        &self,
        input: CreateStationInput,
        data_key: Option<&[u8; 32]>,
    ) -> Result<Station, String> {
        validate_station_fields(
            &input.name,
            &input.station_type,
            &input.website_url,
            input.credit_per_cny,
            input.collection_interval_minutes,
        )?;
        validate_proxy_config(
            input.collector_proxy_mode.clone(),
            input.collector_proxy_url.clone(),
            true,
        )?;

        let connection = self.connection()?;
        create_station_in_connection(&connection, input, data_key)
    }

    pub fn update_station(&self, input: UpdateStationInput) -> Result<Station, String> {
        self.update_station_with_data_key(input, None)
    }

    pub fn update_station_with_data_key(
        &self,
        input: UpdateStationInput,
        data_key: Option<&[u8; 32]>,
    ) -> Result<Station, String> {
        validate_station_fields(
            &input.name,
            &input.station_type,
            &input.website_url,
            input.credit_per_cny,
            input.collection_interval_minutes,
        )?;
        validate_proxy_config(
            input.collector_proxy_mode.clone(),
            input.collector_proxy_url.clone(),
            true,
        )?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始更新站点事务失败: {error}"))?;
        let station = update_station_in_connection(&transaction, input, data_key)?;
        transaction
            .commit()
            .map_err(|error| format!("提交更新站点事务失败: {error}"))?;
        Ok(station)
    }

    pub(crate) fn with_station_endpoint_revision<T>(
        &self,
        station_id: &str,
        expected_revision: i64,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始站点端点事务失败: {error}"))?;
        ensure_station_endpoint_revision(&transaction, station_id, expected_revision)?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| format!("提交站点端点事务失败: {error}"))?;
        Ok(output)
    }

    pub(crate) fn station_endpoint_revision_matches(
        &self,
        station_id: &str,
        expected_revision: i64,
    ) -> Result<bool, String> {
        let connection = self.connection()?;
        Ok(station_endpoint_revision(&connection, station_id)? == expected_revision)
    }

    pub fn delete_station(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        let deleted = connection
            .execute("DELETE FROM stations WHERE id = ?1", params![id])
            .map_err(|error| format!("删除站点失败: {error}"))?;

        if deleted == 0 {
            return Err("站点不存在，无法删除".to_string());
        }

        normalize_station_priorities(&connection)
    }

    pub fn reorder_stations(&self, station_ids: Vec<String>) -> Result<Vec<Station>, String> {
        if station_ids.is_empty() {
            return Err("排序列表不能为空".to_string());
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始排序事务失败: {error}"))?;

        for (index, id) in station_ids.iter().enumerate() {
            let updated = transaction
                .execute(
                    "UPDATE stations SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                    params![index as i64, now_string(), id],
                )
                .map_err(|error| format!("更新站点排序失败: {error}"))?;

            if updated == 0 {
                return Err(format!("站点不存在，无法排序: {id}"));
            }
        }

        transaction
            .commit()
            .map_err(|error| format!("保存站点排序失败: {error}"))?;

        list_stations_from_connection(&connection)
    }

    pub fn get_settings(&self) -> Result<AppSettings, String> {
        let connection = self.connection()?;
        self.settings_from_open_connection(&connection)
    }

    pub fn get_local_access_key(&self) -> Result<String, String> {
        let connection = self.connection()?;
        read_setting(&connection, "local_key")
    }

    pub fn ensure_secure_local_access_key(&self) -> Result<String, String> {
        let connection = self.connection()?;
        let current = read_setting(&connection, "local_key")?;
        if !current.trim().is_empty() && current != INSECURE_LOCAL_KEY_PLACEHOLDER {
            return Ok(current);
        }

        let mut random = [0_u8; 32];
        OsRng.fill_bytes(&mut random);
        let generated = format!("sk-local-{}", URL_SAFE_NO_PAD.encode(random));
        upsert_setting(&connection, "local_key", &generated)?;
        Ok(generated)
    }

    pub fn update_local_access_key(&self, value: String) -> Result<AppSettings, String> {
        let local_key = value.trim();
        if local_key.is_empty() {
            return Err("本地访问密钥不能为空".to_string());
        }

        let connection = self.connection()?;
        upsert_setting(&connection, "local_key", local_key)?;
        self.settings_from_open_connection(&connection)
    }

    pub fn update_settings(&self, input: UpdateSettingsInput) -> Result<AppSettings, String> {
        if input.local_proxy_port == 0 {
            return Err("本地代理端口必须大于 0".to_string());
        }
        if input.low_balance_threshold_cny < 0.0 {
            return Err("低余额阈值不能为负数".to_string());
        }
        if input.collector_interval_minutes == 0 {
            return Err("采集频率必须大于 0 分钟".to_string());
        }
        if input.balance_interval_minutes == 0
            || input.group_rate_interval_minutes == 0
            || input.model_list_interval_minutes == 0
            || input.pricing_refresh_interval_minutes == 0
        {
            return Err("采集周期必须大于 0".to_string());
        }
        if input.collector_timeout_seconds < 3 {
            return Err("采集超时时间不能小于 3 秒".to_string());
        }
        if input.collector_max_concurrency == 0 || input.collector_max_concurrency > 8 {
            return Err("采集并发数必须在 1 到 8 之间".to_string());
        }

        let collector_proxy_mode = validate_proxy_config(
            input.collector_proxy_mode,
            input.collector_proxy_url.clone(),
            false,
        )?;
        let collector_proxy_url = normalize_proxy_url(input.collector_proxy_url);

        let connection = self.connection()?;
        let current_settings = self.settings_from_open_connection(&connection)?;
        let max_rate_multiplier = input
            .max_rate_multiplier
            .unwrap_or(current_settings.max_rate_multiplier);
        let default_routing_group_filter = input
            .default_routing_group_filter
            .unwrap_or(current_settings.default_routing_group_filter);
        let scheduler_advanced_settings = input
            .scheduler_advanced_settings
            .unwrap_or(current_settings.scheduler_advanced_settings);

        if let Some(max_rate_multiplier) = max_rate_multiplier {
            if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
                return Err("invalid max_rate_multiplier".to_string());
            }
        }
        scheduler_advanced_settings
            .validate()
            .map_err(|error| format!("invalid scheduler advanced settings: {error:?}"))?;

        let default_routing_group_filter =
            serialize_routing_group_filter_setting(&default_routing_group_filter)?;
        let scheduler_advanced_settings = serde_json::to_string(&scheduler_advanced_settings)
            .map_err(|error| format!("serialize scheduler advanced settings failed: {error}"))?;
        let values = [
            ("local_proxy_port", input.local_proxy_port.to_string()),
            ("default_routing_strategy", input.default_routing_strategy),
            ("collector_proxy_mode", collector_proxy_mode),
            (
                "collector_proxy_url",
                collector_proxy_url.unwrap_or_default(),
            ),
            (
                "max_rate_multiplier",
                max_rate_multiplier
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ),
            ("default_routing_group_filter", default_routing_group_filter),
            (
                "scheduler_advanced_settings_json",
                scheduler_advanced_settings,
            ),
            (
                "low_balance_threshold_cny",
                input.low_balance_threshold_cny.to_string(),
            ),
            (
                "collector_interval_minutes",
                input.collector_interval_minutes.to_string(),
            ),
            (
                "balance_interval_minutes",
                input.balance_interval_minutes.to_string(),
            ),
            (
                "group_rate_interval_minutes",
                input.group_rate_interval_minutes.to_string(),
            ),
            (
                "model_list_interval_minutes",
                input.model_list_interval_minutes.to_string(),
            ),
            (
                "pricing_refresh_interval_minutes",
                input.pricing_refresh_interval_minutes.to_string(),
            ),
            (
                "collector_timeout_seconds",
                input.collector_timeout_seconds.to_string(),
            ),
            (
                "collector_max_concurrency",
                input.collector_max_concurrency.to_string(),
            ),
            (
                "allow_depleted_fallback",
                input.allow_depleted_fallback.to_string(),
            ),
            (
                "developer_mode_enabled",
                input.developer_mode_enabled.to_string(),
            ),
        ];

        for (key, value) in values {
            upsert_setting(&connection, key, &value)?;
        }

        self.settings_from_open_connection(&connection)
    }

    pub fn set_pending_data_dir(&self, data_dir: PathBuf) -> Result<AppSettings, String> {
        fs::create_dir_all(&data_dir)
            .map_err(|error| format!("无法创建数据目录 {}: {error}", data_dir.display()))?;
        write_data_dir_config(&self.data_dir_config_path, &data_dir, Some(&self.data_dir))?;
        {
            let mut pending = self
                .pending_data_dir
                .lock()
                .map_err(|_| "数据目录配置锁已损坏".to_string())?;
            *pending = Some(data_dir);
        }
        self.get_settings()
    }

    pub fn reset_data_dir_to_default(&self) -> Result<AppSettings, String> {
        fs::create_dir_all(&self.default_data_dir).map_err(|error| {
            format!(
                "无法创建默认数据目录 {}: {error}",
                self.default_data_dir.display()
            )
        })?;
        write_data_dir_config(
            &self.data_dir_config_path,
            &self.default_data_dir,
            Some(&self.data_dir),
        )?;
        {
            let mut pending = self
                .pending_data_dir
                .lock()
                .map_err(|_| "数据目录配置锁已损坏".to_string())?;
            *pending = Some(self.default_data_dir.clone());
        }
        self.get_settings()
    }

    fn settings_from_open_connection(
        &self,
        connection: &Connection,
    ) -> Result<AppSettings, String> {
        let pending_data_dir = self.pending_data_dir_path()?;
        settings_from_connection(
            connection,
            self.data_dir.to_string_lossy().as_ref(),
            pending_data_dir
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        )
    }

    fn pending_data_dir_path(&self) -> Result<Option<PathBuf>, String> {
        let pending = self
            .pending_data_dir
            .lock()
            .map_err(|_| "数据目录配置锁已损坏".to_string())?;
        Ok(pending.as_ref().cloned())
    }

    pub fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        let connection = self.connection()?;
        list_station_keys_from_connection(&connection, &station_id)
    }

    pub fn load_multiplier_source_facts(
        &self,
        station_key_id: &str,
    ) -> Result<MultiplierSourceFacts, String> {
        let connection = self.connection()?;
        load_multiplier_source_facts_from_connection(&connection, station_key_id)
    }

    pub fn load_scheduler_candidates(
        &self,
        filter: &RoutingGroupFilter,
        now_ms: i64,
    ) -> Result<Vec<SchedulerCandidate>, String> {
        let connection = self.connection()?;
        load_scheduler_candidates_from_connection(&connection, filter, now_ms)
    }

    pub fn list_remote_station_keys(
        &self,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, String> {
        let connection = self.connection()?;
        list_remote_station_keys_from_connection(&connection, &station_id)
    }

    pub fn replace_remote_station_keys(
        &self,
        station_id: String,
        keys: Vec<RemoteStationKey>,
    ) -> Result<Vec<RemoteStationKey>, String> {
        let mut connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始远端 Key 发现事务失败: {error}"))?;
        transaction
            .execute(
                "DELETE FROM remote_station_keys WHERE station_id = ?1",
                params![station_id],
            )
            .map_err(|error| format!("清空远端 Key 发现失败: {error}"))?;
        for key in keys {
            insert_remote_station_key(&transaction, &station_id, &key)?;
        }
        transaction
            .commit()
            .map_err(|error| format!("保存远端 Key 发现失败: {error}"))?;
        list_remote_station_keys_from_connection(&connection, &station_id)
    }

    pub fn bind_remote_station_key(
        &self,
        remote_key_id: String,
        station_key_id: String,
    ) -> Result<Vec<RemoteStationKey>, String> {
        let connection = self.connection()?;
        let station_id: Option<String> = connection
            .query_row(
                "SELECT station_id FROM station_keys WHERE id = ?1",
                params![station_key_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("读取 Station Key 失败: {error}"))?;

        let Some(station_id) = station_id else {
            return Err("Station Key 不存在，无法绑定远端 Key".to_string());
        };

        let updated = connection
            .execute(
                "UPDATE remote_station_keys
                    SET match_status = ?1,
                        matched_station_key_id = ?2,
                        match_confidence = 1.0,
                        updated_at = ?3
                  WHERE id = ?4 AND station_id = ?5",
                params![
                    RemoteKeyMatchStatus::Matched.as_str(),
                    station_key_id,
                    now_string(),
                    remote_key_id,
                    station_id,
                ],
            )
            .map_err(|error| format!("绑定远端 Key 失败: {error}"))?;
        if updated == 0 {
            return Err("远端 Key 不存在，无法绑定".to_string());
        }

        list_remote_station_keys_from_connection(&connection, &station_id)
    }

    pub fn unbind_remote_station_key(
        &self,
        remote_key_id: String,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        let updated = connection
            .execute(
                "UPDATE remote_station_keys
                    SET match_status = ?1,
                        matched_station_key_id = NULL,
                        match_confidence = 0.0,
                        updated_at = ?2
                  WHERE id = ?3 AND station_id = ?4",
                params![
                    RemoteKeyMatchStatus::Unbound.as_str(),
                    now_string(),
                    remote_key_id,
                    station_id,
                ],
            )
            .map_err(|error| format!("解绑远端 Key 失败: {error}"))?;
        if updated == 0 {
            return Err("远端 Key 不存在，无法解绑".to_string());
        }

        list_remote_station_keys_from_connection(&connection, &station_id)
    }

    pub fn create_station_key(&self, input: CreateStationKeyInput) -> Result<StationKey, String> {
        let connection = self.connection()?;
        create_station_key_in_connection(&connection, input)
    }

    pub fn create_station_key_with_data_key(
        &self,
        input: CreateStationKeyInput,
        data_key: &[u8; 32],
    ) -> Result<StationKey, String> {
        let connection = self.connection()?;
        create_station_key_in_connection_with_data_key(&connection, input, Some(data_key))
    }

    pub fn update_station_key(&self, input: UpdateStationKeyInput) -> Result<StationKey, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        update_station_key_in_connection(&connection, input)
    }

    pub fn update_station_key_with_data_key(
        &self,
        input: UpdateStationKeyInput,
        data_key: &[u8; 32],
    ) -> Result<StationKey, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        update_station_key_in_connection_with_data_key(&connection, input, Some(data_key))
    }

    pub fn save_station_key_with_defaults(
        &self,
        data_key: &[u8; 32],
        input: SaveStationKeyWithDefaultsInput,
    ) -> Result<SaveStationKeyWithDefaultsResult, String> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始保存 Station Key 默认配置事务失败: {error}"))?;
        let result =
            crate::services::shared_capabilities::save_station_key_with_defaults_in_connection(
                &transaction,
                data_key,
                input,
            )?;
        transaction
            .commit()
            .map_err(|error| format!("提交保存 Station Key 默认配置事务失败: {error}"))?;
        Ok(result)
    }

    pub fn update_station_key_group_binding(
        &self,
        input: UpdateStationKeyGroupBindingInput,
    ) -> Result<StationKey, String> {
        let connection = self.connection()?;
        update_station_key_group_binding_in_connection(&connection, input)
    }

    pub fn touch_station_key_usage(
        &self,
        station_key_id: &str,
        status: &str,
        last_used_at: Option<&str>,
        last_checked_at: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        touch_station_key_usage_in_connection(
            &connection,
            station_key_id,
            status,
            last_used_at,
            last_checked_at,
        )
    }

    pub fn delete_station_key(&self, id: String) -> Result<(), String> {
        let mut connection = self.connection()?;
        let station_id: Option<String> = connection
            .query_row(
                "SELECT station_id FROM station_keys WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("读取 Key 失败: {error}"))?;

        let Some(station_id) = station_id else {
            return Err("Station Key 不存在，无法删除".to_string());
        };

        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始删除 Station Key 事务失败: {error}"))?;
        transaction
            .execute(
                "UPDATE remote_station_keys
                    SET match_status = ?1,
                        matched_station_key_id = NULL,
                        match_confidence = 0.0,
                        updated_at = ?2
                  WHERE matched_station_key_id = ?3",
                params![RemoteKeyMatchStatus::Unbound.as_str(), now_string(), id],
            )
            .map_err(|error| format!("解除远端 Key 绑定失败: {error}"))?;
        transaction
            .execute("DELETE FROM station_keys WHERE id = ?1", params![id])
            .map_err(|error| format!("删除 Station Key 失败: {error}"))?;
        normalize_station_key_priorities(&transaction, &station_id)?;
        transaction
            .commit()
            .map_err(|error| format!("保存删除 Station Key 事务失败: {error}"))?;
        Ok(())
    }

    pub fn reorder_station_keys(
        &self,
        station_id: String,
        key_ids: Vec<String>,
    ) -> Result<Vec<StationKey>, String> {
        if key_ids.is_empty() {
            return Err("Key 排序列表不能为空".to_string());
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始 Key 排序事务失败: {error}"))?;
        for (index, id) in key_ids.iter().enumerate() {
            let updated = transaction
                .execute(
                    "UPDATE station_keys SET priority = ?1, updated_at = ?2 WHERE id = ?3 AND station_id = ?4",
                    params![index as i64, now_string(), id, station_id],
                )
                .map_err(|error| format!("更新 Key 排序失败: {error}"))?;
            if updated == 0 {
                return Err(format!("Station Key 不存在，无法排序: {id}"));
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("保存 Key 排序失败: {error}"))?;
        list_station_keys_from_connection(&connection, &station_id)
    }

    pub fn reorder_local_routing_keys(&self, station_key_ids: Vec<String>) -> Result<(), String> {
        if station_key_ids.is_empty() {
            return Err("Local routing order cannot be empty".to_string());
        }

        let mut seen = HashSet::new();
        for id in &station_key_ids {
            if !seen.insert(id) {
                return Err(format!(
                    "Duplicate Station Key in local routing order: {id}"
                ));
            }
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Start local routing reorder transaction failed: {error}"))?;
        for id in &station_key_ids {
            let exists = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM station_keys WHERE id = ?1)",
                    params![id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| format!("Validate local routing key failed: {error}"))?;
            if exists == 0 {
                return Err(format!(
                    "Station Key does not exist for local routing order: {id}"
                ));
            }
        }

        let remaining_ids = transaction
            .prepare(
                "SELECT id
                   FROM station_keys
                  ORDER BY COALESCE(routing_order, priority) ASC,
                           priority ASC,
                           created_at ASC,
                           id ASC",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| format!("Read remaining local routing keys failed: {error}"))?;
        let requested_ids = station_key_ids.iter().collect::<HashSet<_>>();
        let remaining_ids = remaining_ids
            .into_iter()
            .filter(|id| !requested_ids.contains(id))
            .collect::<Vec<_>>();

        for (index, id) in station_key_ids.iter().enumerate() {
            let updated = transaction
                .execute(
                    "UPDATE station_keys
                        SET routing_order = ?1,
                            updated_at = ?2
                      WHERE id = ?3",
                    params![index as i64, now_string(), id],
                )
                .map_err(|error| format!("Update local routing order failed: {error}"))?;
            if updated == 0 {
                return Err(format!(
                    "Station Key does not exist for local routing order: {id}"
                ));
            }
        }
        for (offset, id) in remaining_ids.iter().enumerate() {
            transaction
                .execute(
                    "UPDATE station_keys
                        SET routing_order = ?1,
                            updated_at = ?2
                      WHERE id = ?3",
                    params![(station_key_ids.len() + offset) as i64, now_string(), id],
                )
                .map_err(|error| format!("Append remaining local routing order failed: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Save local routing order failed: {error}"))?;
        Ok(())
    }

    pub fn list_key_pool_items(&self) -> Result<Vec<KeyPoolItem>, String> {
        let connection = self.connection()?;
        list_key_pool_items_from_connection(&connection)
    }

    pub fn proxy_route_candidates(&self) -> Result<Vec<RouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_route_candidates_from_connection(&connection)
    }

    pub fn proxy_route_candidates_with_data_key(
        &self,
        data_key: &[u8; 32],
    ) -> Result<Vec<RouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_route_candidates_from_connection_with_data_key(&connection, Some(data_key))
    }

    pub fn proxy_rich_route_candidates(&self) -> Result<Vec<RichRouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_rich_route_candidates_from_connection(&connection)
    }

    pub fn proxy_rich_route_candidates_with_data_key(
        &self,
        data_key: &[u8; 32],
    ) -> Result<Vec<RichRouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_rich_route_candidates_from_connection_with_data_key(&connection, Some(data_key))
    }

    pub fn load_local_routing_workspace(
        &self,
        proxy_status: crate::models::proxy::ProxyStatus,
    ) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
        crate::services::proxy::routing_snapshot::load_local_routing_workspace(self, proxy_status)
    }

    pub(crate) fn local_routing_read_candidates(
        &self,
    ) -> Result<Vec<LocalRoutingReadCandidate>, String> {
        let connection = self.connection()?;
        local_routing_read_candidates_from_connection(&connection)
    }

    pub fn route_candidate_economics(
        &self,
        station_key_id: String,
    ) -> Result<Option<RouteCandidateEconomics>, String> {
        let connection = self.connection()?;
        route_candidate_economics_by_station_key(&connection, &station_key_id, None)
    }

    pub fn route_candidate_economics_for_model(
        &self,
        station_key_id: String,
        model: Option<String>,
    ) -> Result<Option<RouteCandidateEconomics>, String> {
        let connection = self.connection()?;
        route_candidate_economics_by_station_key(&connection, &station_key_id, model.as_deref())
    }

    pub fn enabled_model_alias_pairs(&self) -> Result<Vec<(String, String)>, String> {
        let connection = self.connection()?;
        enabled_model_alias_pairs_from_connection(&connection)
    }

    pub fn reorder_key_pool(&self, key_ids: Vec<String>) -> Result<Vec<KeyPoolItem>, String> {
        if key_ids.is_empty() {
            return Err("Key 排序列表不能为空".to_string());
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始 Key 池排序事务失败: {error}"))?;

        for (index, id) in key_ids.iter().enumerate() {
            let updated = transaction
                .execute(
                    "UPDATE station_keys SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                    params![index as i64, now_string(), id],
                )
                .map_err(|error| format!("更新 Key 池排序失败: {error}"))?;
            if updated == 0 {
                return Err(format!("Station Key 不存在，无法排序: {id}"));
            }
        }

        transaction
            .commit()
            .map_err(|error| format!("保存 Key 池排序失败: {error}"))?;

        list_key_pool_items_from_connection(&connection)
    }

    pub fn get_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn get_station_login_password(&self, station_id: String) -> Result<Option<String>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        station_login_password_from_connection(&connection, &station_id)
    }

    pub fn get_station_login_password_with_data_key(
        &self,
        station_id: String,
        data_key: &[u8; 32],
    ) -> Result<Option<String>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        station_login_password_from_connection_with_data_key(
            &connection,
            &station_id,
            Some(data_key),
        )
    }

    pub fn update_station_credentials(
        &self,
        input: UpdateStationCredentialsInput,
    ) -> Result<StationCredentials, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        let station_id = input.station_id.clone();
        upsert_station_credentials(&connection, input)?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn update_station_credentials_with_data_key(
        &self,
        input: UpdateStationCredentialsInput,
        data_key: &[u8; 32],
    ) -> Result<StationCredentials, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        let station_id = input.station_id.clone();
        upsert_station_credentials_with_data_key(&connection, input, Some(data_key))?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn update_station_session_with_data_key(
        &self,
        input: UpdateStationSessionInput,
        data_key: &[u8; 32],
    ) -> Result<StationCredentials, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        let station_id = input.station_id.clone();
        upsert_station_session_with_data_key(&connection, input, data_key)?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn persist_station_session_with_data_key(
        &self,
        input: PersistStationSessionInput,
        data_key: &[u8; 32],
    ) -> Result<StationCredentials, String> {
        let mut connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        let station_id = input.station_id.clone();
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始保存 session 事务失败: {error}"))?;
        persist_station_session_from_connection(&transaction, input, data_key)?;
        transaction
            .commit()
            .map_err(|error| format!("提交保存 session 事务失败: {error}"))?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn persist_station_session_if_revision(
        &self,
        input: PersistStationSessionInput,
        expected_revision: i64,
        data_key: &[u8; 32],
    ) -> Result<StationCredentials, String> {
        let station_id = input.station_id.clone();
        self.with_station_endpoint_revision(&station_id, expected_revision, |transaction| {
            persist_station_session_from_connection(transaction, input, data_key)
        })?;
        let connection = self.connection()?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn resolve_station_session_with_data_key(
        &self,
        station_id: String,
        data_key: &[u8; 32],
        now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        resolve_station_session_from_connection(&connection, &station_id, data_key, now_ms)
    }

    pub fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String> {
        let mut connection = self.connection()?;
        validate_station_exists(&connection, station_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开始失效 session 事务失败: {error}"))?;
        invalidate_station_session_credential_from_connection(&transaction, station_id, kind)?;
        transaction
            .commit()
            .map_err(|error| format!("提交失效 session 事务失败: {error}"))
    }

    pub fn clear_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        connection
            .execute(
                "DELETE FROM station_credentials WHERE station_id = ?1",
                params![station_id],
            )
            .map_err(|error| format!("清除登录信息失败: {error}"))?;
        station_credentials_from_connection(&connection, &station_id)
    }

    pub fn insert_collector_snapshot(
        &self,
        station_id: &str,
        source: &str,
        status: &str,
        summary_json: Value,
        normalized_json: Value,
        raw_json_redacted: Option<Value>,
        error_message: Option<String>,
    ) -> Result<CollectorSnapshot, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, station_id)?;
        insert_collector_snapshot_in_connection(
            &connection,
            station_id,
            source,
            status,
            summary_json,
            normalized_json,
            raw_json_redacted,
            error_message,
        )
    }

    pub(crate) fn insert_collector_snapshot_for_revision(
        &self,
        station_id: &str,
        endpoint_revision: i64,
        source: &str,
        status: &str,
        summary_json: Value,
        normalized_json: Value,
        raw_json_redacted: Option<Value>,
        error_message: Option<String>,
        emit_change_events: bool,
    ) -> Result<CollectorSnapshot, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, station_id)?;
        insert_collector_snapshot_in_connection_with_revision(
            &connection,
            station_id,
            endpoint_revision,
            source,
            status,
            summary_json,
            normalized_json,
            raw_json_redacted,
            error_message,
            emit_change_events,
        )
    }

    pub fn list_collector_snapshots(
        &self,
        station_id: String,
    ) -> Result<Vec<CollectorSnapshot>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        list_collector_snapshots_from_connection(&connection, &station_id)
    }

    pub fn get_latest_collector_snapshot(
        &self,
        station_id: String,
    ) -> Result<Option<CollectorSnapshot>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        latest_collector_snapshot_from_connection(&connection, &station_id)
    }

    pub fn insert_request_log(&self, input: CreateRequestLogInput) -> Result<RequestLog, String> {
        let connection = self.connection()?;
        insert_request_log_in_connection(&connection, input)
    }

    pub fn list_request_logs(&self) -> Result<Vec<RequestLog>, String> {
        let connection = self.connection()?;
        list_request_logs_from_connection(&connection)
    }

    pub fn list_local_proxy_request_logs(&self) -> Result<Vec<RequestLog>, String> {
        let connection = self.connection()?;
        list_local_proxy_request_logs_from_connection(&connection)
    }

    pub fn clear_request_logs(&self) -> Result<(), String> {
        let connection = self.connection()?;
        connection
            .execute("DELETE FROM request_logs", [])
            .map_err(|error| format!("清空请求日志失败: {error}"))?;
        Ok(())
    }

    pub fn list_pricing_rules(&self) -> Result<Vec<PricingRule>, String> {
        let connection = self.connection()?;
        list_pricing_rules_from_connection(&connection)
    }

    pub fn list_model_base_prices(&self) -> Result<Vec<ModelBasePrice>, String> {
        let connection = self.connection()?;
        list_model_base_prices_from_connection(&connection)
    }

    pub fn upsert_model_base_price(
        &self,
        input: UpsertModelBasePriceInput,
    ) -> Result<ModelBasePrice, String> {
        let connection = self.connection()?;
        upsert_model_base_price_in_connection(&connection, input)
    }

    pub fn reset_model_base_prices_to_builtins(&self) -> Result<Vec<ModelBasePrice>, String> {
        let connection = self.connection()?;
        seed_builtin_model_base_prices(&connection)
            .map_err(|error| format!("恢复内置模型基准价格失败: {error}"))?;
        list_model_base_prices_from_connection(&connection)
    }

    pub fn upsert_pricing_rule(
        &self,
        input: UpsertPricingRuleInput,
    ) -> Result<PricingRule, String> {
        let connection = self.connection()?;
        upsert_pricing_rule_in_connection(&connection, input)
    }

    pub fn delete_pricing_rule(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_pricing_rule_from_connection(&connection, &id)
    }

    pub fn list_balance_snapshots(&self) -> Result<Vec<BalanceSnapshot>, String> {
        let connection = self.connection()?;
        list_balance_snapshots_from_connection(&connection)
    }

    pub fn list_current_station_balance_snapshots(&self) -> Result<Vec<BalanceSnapshot>, String> {
        let connection = self.connection()?;
        list_current_station_balance_snapshots_from_connection(&connection)
    }

    pub fn list_balance_snapshots_for_station(
        &self,
        station_id: String,
    ) -> Result<Vec<BalanceSnapshot>, String> {
        let connection = self.connection()?;
        list_balance_snapshots_for_station_from_connection(&connection, &station_id)
    }

    pub fn upsert_balance_snapshot(
        &self,
        input: UpsertBalanceSnapshotInput,
    ) -> Result<BalanceSnapshot, String> {
        let connection = self.connection()?;
        upsert_balance_snapshot_in_connection(&connection, input)
    }

    pub fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        let connection = self.connection()?;
        list_station_group_bindings_from_connection(&connection, &station_id)
    }

    pub fn upsert_station_group_binding(
        &self,
        input: UpsertStationGroupBindingInput,
    ) -> Result<StationGroupBinding, String> {
        let connection = self.connection()?;
        upsert_station_group_binding_in_connection(&connection, input)
    }

    pub fn mark_missing_station_group_bindings(
        &self,
        station_id: &str,
        rate_sources: Vec<String>,
        present_group_key_hashes: Vec<String>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        mark_missing_station_group_bindings_in_connection(
            &connection,
            station_id,
            rate_sources,
            present_group_key_hashes,
        )
    }

    pub fn list_group_rate_records(
        &self,
        station_id: String,
    ) -> Result<Vec<GroupRateRecord>, String> {
        let connection = self.connection()?;
        list_group_rate_records_from_connection(&connection, &station_id)
    }

    pub fn list_station_group_options(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupOption>, String> {
        let bindings = self.list_station_group_bindings(station_id.clone())?;
        let rates = self.list_group_rate_records(station_id)?;
        Ok(crate::services::shared_capabilities::station_group_options_from_facts(bindings, rates))
    }

    pub fn upsert_group_rate_record_if_changed(
        &self,
        input: InsertGroupRateRecordInput,
    ) -> Result<Option<GroupRateRecord>, String> {
        let connection = self.connection()?;
        insert_group_rate_record_if_changed_in_connection(&connection, input)
    }

    pub fn list_collector_runs(&self, station_id: String) -> Result<Vec<CollectorRun>, String> {
        let connection = self.connection()?;
        list_collector_runs_from_connection(&connection, &station_id)
    }

    pub fn seed_builtin_channel_monitor_templates(&self) -> Result<(), String> {
        let connection = self.connection()?;
        seed_builtin_channel_monitor_templates_in_connection(&connection)
            .map_err(|error| format!("初始化通道监控模板失败: {error}"))
    }

    pub fn list_channel_monitor_templates(
        &self,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, String> {
        let connection = self.connection()?;
        list_channel_monitor_templates_from_connection(&connection)
    }

    pub fn get_channel_monitor_template(
        &self,
        id: &str,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        channel_monitor_template_by_id(&connection, id)
    }

    pub fn create_channel_monitor_template(
        &self,
        input: CreateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        create_channel_monitor_template_in_connection(&connection, input)
    }

    pub fn update_channel_monitor_template(
        &self,
        input: UpdateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        update_channel_monitor_template_in_connection(&connection, input)
    }

    pub fn duplicate_channel_monitor_template(
        &self,
        id: String,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        duplicate_channel_monitor_template_in_connection(&connection, &id)
    }

    pub fn delete_channel_monitor_template(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_channel_monitor_template_in_connection(&connection, &id)
    }

    pub fn list_channel_monitors(&self) -> Result<Vec<ChannelMonitor>, String> {
        let connection = self.connection()?;
        list_channel_monitors_from_connection(&connection)
    }

    pub fn list_channel_monitor_summaries(
        &self,
        run_since: Option<&str>,
        run_limit: Option<usize>,
    ) -> Result<Vec<ChannelMonitorSummary>, String> {
        let monitors = self.list_channel_monitors()?;
        Ok(
            crate::services::shared_capabilities::channel_monitor_summaries_from_database(
                self, monitors, run_since, run_limit,
            ),
        )
    }

    pub fn list_channel_status_summaries(&self) -> Result<Vec<ChannelStatusSummary>, String> {
        let monitors = self.list_channel_monitors()?;
        crate::services::shared_capabilities::channel_status_summaries_from_database(self, monitors)
    }

    pub fn get_channel_monitor(&self, id: &str) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        channel_monitor_by_id(&connection, id)
    }

    pub fn create_channel_monitor(
        &self,
        input: CreateChannelMonitorInput,
    ) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        create_channel_monitor_in_connection(&connection, input)
    }

    pub fn update_channel_monitor(
        &self,
        input: UpdateChannelMonitorInput,
    ) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        update_channel_monitor_in_connection(&connection, input)
    }

    pub fn delete_channel_monitor(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_channel_monitor_in_connection(&connection, &id)
    }

    pub fn list_channel_monitor_runs(
        &self,
        monitor_id: String,
    ) -> Result<Vec<ChannelMonitorRun>, String> {
        let connection = self.connection()?;
        list_channel_monitor_runs_from_connection(&connection, &monitor_id)
    }

    pub fn list_channel_monitor_runs_for_summary(
        &self,
        monitor_id: String,
        run_since: Option<&str>,
        run_limit: Option<usize>,
    ) -> Result<Vec<ChannelMonitorRun>, String> {
        let connection = self.connection()?;
        list_channel_monitor_runs_for_summary_from_connection(
            &connection,
            &monitor_id,
            run_since,
            run_limit,
        )
    }

    pub fn channel_status_window_facts(
        &self,
        monitor_id: &str,
        since_ms: Option<i64>,
        timeline_limit: usize,
    ) -> Result<ChannelStatusWindowFacts, String> {
        let connection = self.connection()?;
        channel_status_window_facts_from_connection(
            &connection,
            monitor_id,
            since_ms,
            timeline_limit,
        )
    }

    pub fn insert_channel_monitor_run(
        &self,
        input: CreateChannelMonitorRunInput,
    ) -> Result<ChannelMonitorRun, String> {
        let connection = self.connection()?;
        insert_channel_monitor_run_in_connection(&connection, input)
    }

    pub fn update_channel_monitor_after_run(
        &self,
        id: &str,
        status: &str,
        finished_at: &str,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        update_channel_monitor_after_run_in_connection(
            &connection,
            id,
            status,
            finished_at,
            error_message,
        )
    }

    pub fn schedule_next_channel_monitor_run(&self, id: &str) -> Result<String, String> {
        let connection = self.connection()?;
        schedule_next_channel_monitor_run_in_connection(&connection, id)
    }

    pub fn due_channel_monitors(&self, now: &str) -> Result<Vec<ChannelMonitor>, String> {
        let connection = self.connection()?;
        due_channel_monitors_from_connection(&connection, now)
    }

    pub fn due_station_collectors(&self, now: &str) -> Result<Vec<Station>, String> {
        let connection = self.connection()?;
        due_station_collectors_from_connection(&connection, now)
    }

    pub fn create_collector_run(
        &self,
        input: CreateCollectorRunInput,
    ) -> Result<CollectorRun, String> {
        let connection = self.connection()?;
        create_collector_run_in_connection(&connection, input)
    }

    pub(crate) fn create_collector_run_for_revision(
        &self,
        input: CreateCollectorRunInput,
        endpoint_revision: i64,
    ) -> Result<CollectorRun, String> {
        let connection = self.connection()?;
        create_collector_run_in_connection_with_revision(&connection, input, endpoint_revision)
    }

    pub fn finish_collector_run(
        &self,
        input: FinishCollectorRunInput,
    ) -> Result<CollectorRun, String> {
        let connection = self.connection()?;
        finish_collector_run_in_connection(&connection, input)
    }

    pub fn list_change_events(&self) -> Result<Vec<ChangeEvent>, String> {
        let connection = self.connection()?;
        list_change_events_from_connection(&connection)
    }

    pub fn clear_change_events(&self) -> Result<(), String> {
        let connection = self.connection()?;
        connection
            .execute("DELETE FROM change_events", [])
            .map_err(|error| format!("清除变更事件失败: {error}"))?;
        Ok(())
    }

    pub fn list_change_events_for_station(
        &self,
        station_id: String,
    ) -> Result<Vec<ChangeEvent>, String> {
        let connection = self.connection()?;
        list_change_events_for_station_from_connection(&connection, &station_id)
    }

    pub fn upsert_change_event(
        &self,
        input: UpsertChangeEventInput,
    ) -> Result<ChangeEvent, String> {
        let connection = self.connection()?;
        upsert_change_event_in_connection(&connection, input)
    }

    pub fn mark_change_event_read(&self, id: String) -> Result<ChangeEvent, String> {
        let connection = self.connection()?;
        update_change_event_status_in_connection(&connection, &id, STATUS_READ)
    }

    pub fn dismiss_change_event(&self, id: String) -> Result<ChangeEvent, String> {
        let connection = self.connection()?;
        update_change_event_status_in_connection(&connection, &id, STATUS_DISMISSED)
    }

    pub fn resolve_change_event(&self, id: String) -> Result<ChangeEvent, String> {
        let connection = self.connection()?;
        resolve_change_event_in_connection(&connection, &id)
    }

    pub fn update_station_login_status(
        &self,
        station_id: &str,
        login_status: &str,
        login_error: Option<String>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        update_station_login_status_in_connection(
            &connection,
            station_id,
            login_status,
            login_error,
        )
    }

    pub fn station_for_collector(&self, station_id: &str) -> Result<Station, String> {
        let connection = self.connection()?;
        station_by_id(&connection, station_id)
    }

    pub fn get_station_key_capabilities(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyCapabilities, String> {
        let connection = self.connection()?;
        station_key_capabilities_by_id(&connection, &station_key_id)
    }

    pub fn update_station_key_capabilities(
        &self,
        input: UpdateStationKeyCapabilitiesInput,
    ) -> Result<StationKeyCapabilities, String> {
        let connection = self.connection()?;
        update_station_key_capabilities_in_connection(&connection, input)
    }

    pub fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, String> {
        let connection = self.connection()?;
        list_model_aliases_from_connection(&connection)
    }

    pub fn upsert_model_alias(&self, input: UpsertModelAliasInput) -> Result<ModelAlias, String> {
        let connection = self.connection()?;
        upsert_model_alias_in_connection(&connection, input)
    }

    pub fn delete_model_alias(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_model_alias_in_connection(&connection, &id)
    }

    pub fn list_station_key_health(&self) -> Result<Vec<StationKeyHealth>, String> {
        let connection = self.connection()?;
        list_station_key_health_from_connection(&connection)
    }

    pub fn get_station_key_health(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyHealth, String> {
        let connection = self.connection()?;
        station_key_health_by_id(&connection, &station_key_id)
    }

    pub fn list_station_endpoint_health(&self) -> Result<Vec<StationEndpointHealth>, String> {
        let connection = self.connection()?;
        list_station_endpoint_health_from_connection(&connection)
    }

    pub fn get_station_endpoint_health(
        &self,
        station_id: String,
    ) -> Result<StationEndpointHealth, String> {
        let connection = self.connection()?;
        station_endpoint_health_by_id(&connection, &station_id)
    }

    pub fn upsert_station_endpoint_health(
        &self,
        station_id: &str,
        status: &str,
        latency_ms: Option<i64>,
        checked_at: &str,
        error_summary: Option<&str>,
    ) -> Result<StationEndpointHealth, String> {
        let connection = self.connection()?;
        upsert_station_endpoint_health_in_connection(
            &connection,
            station_id,
            status,
            latency_ms,
            checked_at,
            error_summary,
        )
    }

    pub fn record_station_key_success(
        &self,
        station_key_id: &str,
        duration_ms: i64,
        now: &str,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        record_station_key_success_in_connection(&connection, station_key_id, duration_ms, now)
    }

    pub fn record_station_key_failure(
        &self,
        station_key_id: &str,
        error_summary: &str,
        now: &str,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        record_station_key_failure_in_connection(&connection, station_key_id, error_summary, now, 3)
    }

    pub fn record_station_key_failure_with_threshold(
        &self,
        station_key_id: &str,
        error_summary: &str,
        now: &str,
        consecutive_failure_threshold: i64,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        record_station_key_failure_in_connection(
            &connection,
            station_key_id,
            error_summary,
            now,
            consecutive_failure_threshold,
        )
    }

    pub fn record_station_key_failure_with_cooldown(
        &self,
        station_key_id: &str,
        error_summary: &str,
        now: &str,
        cooldown_until: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        record_station_key_failure_with_explicit_cooldown_in_connection(
            &connection,
            station_key_id,
            error_summary,
            now,
            cooldown_until,
        )
    }

    pub fn simulate_route(
        &self,
        input: RouteSimulationInput,
    ) -> Result<RouteSimulationResult, String> {
        let connection = self.connection()?;
        simulate_route_in_connection(&connection, self.db_path.to_string_lossy().as_ref(), input)
    }
}

fn initialize_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS stations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            station_type TEXT NOT NULL,
            website_url TEXT NOT NULL,
            api_base_url TEXT NOT NULL,
            endpoint_revision INTEGER NOT NULL DEFAULT 1,
            api_key TEXT NOT NULL,
            collector_proxy_mode TEXT NOT NULL DEFAULT 'inherit',
            collector_proxy_url TEXT,
            upstream_api_format TEXT NOT NULL DEFAULT 'auto',
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            credit_per_cny REAL NOT NULL DEFAULT 1,
            balance_raw REAL,
            balance_cny REAL,
            low_balance_threshold_cny REAL,
            collection_interval_minutes INTEGER NOT NULL DEFAULT 5,
            status TEXT NOT NULL DEFAULT 'unchecked',
            latency_ms INTEGER,
            last_checked_at TEXT,
            last_pricing_fetched_at TEXT,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_stations_priority
            ON stations(priority ASC, created_at ASC);

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS station_credentials (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL UNIQUE,
            login_username TEXT,
            login_password TEXT,
            remember_password INTEGER NOT NULL DEFAULT 0,
            login_status TEXT NOT NULL DEFAULT 'unknown',
            login_error TEXT,
            last_login_at TEXT,
            session_status TEXT NOT NULL DEFAULT 'none',
            session_expires_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS station_keys (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            name TEXT NOT NULL,
            api_key TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            routing_order INTEGER,
            max_concurrency INTEGER NOT NULL DEFAULT 3,
            load_factor INTEGER,
            schedulable INTEGER NOT NULL DEFAULT 1,
            group_name TEXT,
            tier_label TEXT,
            status TEXT NOT NULL DEFAULT 'unchecked',
            manual_rate_multiplier REAL,
            manual_rate_updated_at TEXT,
            last_checked_at TEXT,
            last_used_at TEXT,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_station_keys_station_priority
            ON station_keys(station_id, priority ASC, created_at ASC);

        CREATE TABLE IF NOT EXISTS channel_monitor_request_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            endpoint_kind TEXT NOT NULL,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            request_body_json TEXT NOT NULL CHECK(json_valid(request_body_json)),
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
            built_in INTEGER NOT NULL DEFAULT 0 CHECK(built_in IN (0, 1)),
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_templates_enabled
            ON channel_monitor_request_templates(enabled, built_in, updated_at DESC);

        CREATE TABLE IF NOT EXISTS channel_monitors (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            target_type TEXT NOT NULL CHECK(target_type IN ('station_key', 'station')),
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            template_id TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
            interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 15 AND 3600),
            jitter_seconds INTEGER NOT NULL DEFAULT 0 CHECK(jitter_seconds BETWEEN 0 AND 600),
            timeout_seconds INTEGER NOT NULL CHECK(timeout_seconds BETWEEN 5 AND 120),
            max_concurrency INTEGER NOT NULL DEFAULT 1 CHECK(max_concurrency BETWEEN 1 AND 16),
            consecutive_failure_threshold INTEGER NOT NULL DEFAULT 3 CHECK(consecutive_failure_threshold BETWEEN 1 AND 20),
            fallback_models_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(fallback_models_json) AND json_type(fallback_models_json) = 'array'),
            last_run_at TEXT,
            next_run_at TEXT,
            last_status TEXT CHECK(last_status IS NULL OR last_status IN ('success', 'warning', 'failed', 'skipped')),
            last_error_message TEXT,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            CHECK(interval_seconds - jitter_seconds >= 15),
            CHECK(
                (target_type = 'station_key' AND station_key_id IS NOT NULL)
                OR (target_type = 'station' AND station_key_id IS NULL)
            ),
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES channel_monitor_request_templates(id)
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitors_enabled_target
            ON channel_monitors(enabled, target_type, station_id, station_key_id);

        CREATE INDEX IF NOT EXISTS idx_channel_monitors_template
            ON channel_monitors(template_id);

        CREATE TABLE IF NOT EXISTS channel_monitor_runs (
            id TEXT PRIMARY KEY,
            monitor_id TEXT NOT NULL,
            template_id TEXT NOT NULL,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            status TEXT NOT NULL CHECK(status IN ('success', 'warning', 'failed', 'skipped')),
            started_at TEXT NOT NULL CHECK(TRIM(started_at) != ''),
            finished_at TEXT,
            duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms >= 0),
            http_status INTEGER CHECK(http_status IS NULL OR http_status BETWEEN 100 AND 599),
            latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0),
            response_model TEXT,
            fallback_model TEXT,
            error_message TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(monitor_id) REFERENCES channel_monitors(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES channel_monitor_request_templates(id),
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_runs_monitor_created
            ON channel_monitor_runs(monitor_id, created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_runs_monitor_started_at
            ON channel_monitor_runs(monitor_id, CAST(started_at AS INTEGER) DESC);

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_runs_station_created
            ON channel_monitor_runs(station_id, station_key_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS collector_snapshots (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            endpoint_revision INTEGER NOT NULL DEFAULT 1,
            source TEXT NOT NULL,
            status TEXT NOT NULL,
            fetched_at TEXT NOT NULL,
            summary_json TEXT NOT NULL,
            normalized_json TEXT NOT NULL,
            raw_json_redacted TEXT,
            error_message TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_collector_snapshots_station_created
            ON collector_snapshots(station_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS pricing_rules (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            group_name TEXT,
            tier_label TEXT,
            model TEXT NOT NULL,
            input_price REAL,
            output_price REAL,
            fixed_price REAL,
            currency TEXT NOT NULL,
            unit TEXT NOT NULL,
            price_type TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.5,
            enabled INTEGER NOT NULL DEFAULT 1,
            note TEXT,
            collected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_pricing_rules_station_model
            ON pricing_rules(station_id, model, enabled, updated_at DESC);

        CREATE TABLE IF NOT EXISTS model_base_prices (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            input_price REAL,
            output_price REAL,
            currency TEXT NOT NULL,
            unit TEXT NOT NULL,
            source_url TEXT NOT NULL,
            source_label TEXT NOT NULL,
            source_checked_at TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            built_in INTEGER NOT NULL DEFAULT 0,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_model_base_prices_model
            ON model_base_prices(model, enabled, updated_at DESC);

        CREATE TABLE IF NOT EXISTS balance_snapshots (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            scope TEXT NOT NULL,
            value REAL,
            currency TEXT NOT NULL,
            credit_unit TEXT,
            used_value REAL,
            total_value REAL,
            today_request_count INTEGER,
            total_request_count INTEGER,
            today_consumption REAL,
            total_consumption REAL,
            today_base_consumption REAL,
            total_base_consumption REAL,
            today_token_count INTEGER,
            total_token_count INTEGER,
            today_input_token_count INTEGER,
            today_output_token_count INTEGER,
            total_input_token_count INTEGER,
            total_output_token_count INTEGER,
            account_concurrency_limit INTEGER,
            low_balance_threshold REAL,
            status TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.5,
            collected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_balance_snapshots_station_scope_updated
            ON balance_snapshots(station_id, scope, updated_at DESC);

        CREATE TABLE IF NOT EXISTS request_logs (
            id TEXT PRIMARY KEY,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            duration_ms INTEGER,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            model TEXT,
            stream INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL,
            lifecycle_status TEXT,
            station_key_id TEXT,
            station_id TEXT,
            upstream_base_url TEXT,
            fallback_count INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            route_policy TEXT,
            route_reason TEXT,
            rejected_candidates_json TEXT,
            prompt_tokens INTEGER,
            completion_tokens INTEGER,
            total_tokens INTEGER,
            cache_creation_tokens INTEGER,
            cache_read_tokens INTEGER,
            reasoning_effort TEXT,
            first_token_ms INTEGER,
            billing_mode TEXT,
            estimated_input_cost REAL,
            estimated_output_cost REAL,
            estimated_total_cost REAL,
            base_input_cost REAL,
            base_output_cost REAL,
            base_fixed_cost REAL,
            base_total_cost REAL,
            cost_currency TEXT,
            pricing_rule_id TEXT,
            pricing_source TEXT,
            cost_status TEXT,
            group_binding_id TEXT,
            normalization_status TEXT,
            balance_scope TEXT,
            economic_context_json TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_request_logs_created
            ON request_logs(created_at DESC);

        CREATE TABLE IF NOT EXISTS station_key_capabilities (
            station_key_id TEXT PRIMARY KEY,
            supports_chat_completions INTEGER NOT NULL DEFAULT 1,
            supports_responses INTEGER NOT NULL DEFAULT 1,
            supports_embeddings INTEGER NOT NULL DEFAULT 0,
            supports_stream INTEGER NOT NULL DEFAULT 1,
            supports_tools INTEGER NOT NULL DEFAULT 0,
            supports_vision INTEGER NOT NULL DEFAULT 0,
            supports_reasoning INTEGER NOT NULL DEFAULT 0,
            model_allowlist_json TEXT NOT NULL DEFAULT '[]',
            model_blocklist_json TEXT NOT NULL DEFAULT '[]',
            preferred_models_json TEXT NOT NULL DEFAULT '[]',
            only_use_as_backup INTEGER NOT NULL DEFAULT 0,
            routing_tags_json TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS model_aliases (
            id TEXT PRIMARY KEY,
            client_model TEXT NOT NULL,
            upstream_model TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_model_aliases_client_upstream
            ON model_aliases(client_model, upstream_model);

        CREATE TABLE IF NOT EXISTS station_key_health (
            station_key_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL DEFAULT 1,
            last_success_at TEXT,
            last_failure_at TEXT,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            total_duration_ms INTEGER NOT NULL DEFAULT 0,
            avg_latency_ms INTEGER,
            last_error_summary TEXT,
            cooldown_until TEXT,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS station_endpoint_health (
            station_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL CHECK(status IN ('unchecked', 'success', 'failed')),
            latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0),
            checked_at TEXT,
            error_summary TEXT,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS change_events (
            id TEXT PRIMARY KEY,
            severity TEXT NOT NULL,
            event_type TEXT NOT NULL,
            status TEXT NOT NULL,
            title TEXT NOT NULL,
            message TEXT NOT NULL,
            object_type TEXT NOT NULL,
            object_id TEXT,
            station_id TEXT,
            station_key_id TEXT,
            pricing_rule_id TEXT,
            request_log_id TEXT,
            old_value_json TEXT,
            new_value_json TEXT,
            impact_json TEXT,
            dedupe_key TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL,
            detected_at TEXT NOT NULL,
            resolved_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_change_events_status_severity_updated
            ON change_events(status, severity, updated_at DESC);

        CREATE INDEX IF NOT EXISTS idx_change_events_station_updated
            ON change_events(station_id, updated_at DESC);

        CREATE INDEX IF NOT EXISTS idx_change_events_station_key_updated
            ON change_events(station_key_id, updated_at DESC);

        CREATE TABLE IF NOT EXISTS secrets (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            ciphertext TEXT NOT NULL,
            nonce TEXT NOT NULL,
            aad TEXT NOT NULL,
            masked_value TEXT NOT NULL,
            value_hash TEXT NOT NULL,
            encryption_version INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_secrets_owner_kind
            ON secrets(owner_id, kind);

        CREATE TABLE IF NOT EXISTS secret_migration_events (
            id TEXT PRIMARY KEY,
            owner_table TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            secret_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            error_message TEXT,
            created_at TEXT NOT NULL
        );
        ",
    )?;
    migrate_p9_fact_schema(connection)?;
    migrate_channel_monitor_runtime_columns(connection)?;
    seed_builtin_channel_monitor_templates_in_connection(connection)
}

fn migrate_channel_monitor_runtime_columns(connection: &Connection) -> rusqlite::Result<()> {
    add_column_if_missing(connection, "channel_monitors", "last_run_at", "TEXT")?;
    add_column_if_missing(connection, "channel_monitors", "next_run_at", "TEXT")?;
    add_column_if_missing(connection, "channel_monitors", "last_status", "TEXT")?;
    add_column_if_missing(connection, "channel_monitors", "last_error_message", "TEXT")?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_channel_monitors_due
            ON channel_monitors(enabled, next_run_at)",
        [],
    )?;
    Ok(())
}

fn migrate_p9_fact_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS station_group_bindings (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            binding_kind TEXT NOT NULL,
            parent_group_binding_id TEXT,
            group_key_hash TEXT NOT NULL,
            group_id_hash TEXT,
            group_id_enc TEXT,
            group_name TEXT NOT NULL,
            binding_status TEXT NOT NULL,
            default_rate_multiplier REAL,
            user_rate_multiplier REAL,
            effective_rate_multiplier REAL,
            inferred_group_category TEXT,
            group_category_override TEXT,
            rate_source TEXT,
            confidence REAL NOT NULL DEFAULT 0.5,
            last_seen_at TEXT,
            last_checked_at TEXT,
            last_rate_changed_at TEXT,
            raw_json_redacted TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
            FOREIGN KEY(parent_group_binding_id) REFERENCES station_group_bindings(id) ON DELETE SET NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_group_bindings_station_group_key
            ON station_group_bindings(station_id, binding_kind, group_key_hash)
            WHERE binding_kind = 'station_group';

        CREATE UNIQUE INDEX IF NOT EXISTS idx_group_bindings_key_group_key
            ON station_group_bindings(station_key_id, binding_kind, group_key_hash)
            WHERE binding_kind = 'key_binding';

        CREATE INDEX IF NOT EXISTS idx_group_bindings_station_status
            ON station_group_bindings(station_id, binding_status, updated_at DESC);

        CREATE TABLE IF NOT EXISTS group_rate_records (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            group_binding_id TEXT,
            binding_kind TEXT NOT NULL,
            group_key_hash TEXT NOT NULL,
            group_name TEXT NOT NULL,
            default_rate_multiplier REAL,
            user_rate_multiplier REAL,
            effective_rate_multiplier REAL,
            inferred_group_category TEXT,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.5,
            raw_json_redacted TEXT,
            checked_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
            FOREIGN KEY(group_binding_id) REFERENCES station_group_bindings(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_group_rate_records_binding_checked
            ON group_rate_records(group_binding_id, checked_at DESC);

        CREATE INDEX IF NOT EXISTS idx_group_rate_records_station_checked
            ON group_rate_records(station_id, checked_at DESC);

        CREATE TABLE IF NOT EXISTS collector_runs (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            endpoint_revision INTEGER NOT NULL DEFAULT 1,
            parent_run_id TEXT,
            adapter TEXT NOT NULL,
            task_type TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            duration_ms INTEGER,
            endpoint_count INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            manual_action_required INTEGER NOT NULL DEFAULT 0,
            error_code TEXT,
            error_message TEXT,
            snapshot_id TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(parent_run_id) REFERENCES collector_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(snapshot_id) REFERENCES collector_snapshots(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_collector_runs_station_created
            ON collector_runs(station_id, created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_collector_runs_parent
            ON collector_runs(parent_run_id, created_at ASC);
        "#,
    )?;

    add_column_if_missing(
        connection,
        "station_credentials",
        "access_token_secret_id",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "refresh_token_secret_id",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "cookie_secret_id",
        "TEXT",
    )?;
    add_column_if_missing(connection, "station_credentials", "newapi_user_id", "TEXT")?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "token_expires_at",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "token_refreshed_at",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "session_source",
        "TEXT NOT NULL DEFAULT 'none'",
    )?;

    add_column_if_missing(connection, "station_keys", "group_binding_id", "TEXT")?;
    add_column_if_missing(connection, "station_keys", "group_id_hash", "TEXT")?;
    add_column_if_missing(connection, "station_keys", "rate_multiplier", "REAL")?;
    add_column_if_missing(connection, "station_keys", "rate_source", "TEXT")?;
    add_column_if_missing(connection, "station_keys", "rate_collected_at", "TEXT")?;
    add_column_if_missing(connection, "station_keys", "balance_scope", "TEXT")?;
    add_column_if_missing(connection, "station_keys", "routing_order", "INTEGER")?;
    initialize_local_routing_order(connection)?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_station_keys_routing_order
            ON station_keys(routing_order ASC, priority ASC, created_at ASC, id ASC)",
        [],
    )?;

    add_column_if_missing(connection, "pricing_rules", "station_key_id", "TEXT")?;
    add_column_if_missing(connection, "pricing_rules", "group_binding_id", "TEXT")?;
    add_column_if_missing(connection, "pricing_rules", "rate_multiplier", "REAL")?;
    add_column_if_missing(connection, "pricing_rules", "base_price_source", "TEXT")?;
    add_column_if_missing(
        connection,
        "pricing_rules",
        "normalization_status",
        "TEXT NOT NULL DEFAULT 'manual'",
    )?;
    add_column_if_missing(connection, "pricing_rules", "valid_from", "TEXT")?;
    add_column_if_missing(connection, "pricing_rules", "valid_until", "TEXT")?;
    add_column_if_missing(
        connection,
        "station_group_bindings",
        "inferred_group_category",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "station_group_bindings",
        "group_category_override",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "group_rate_records",
        "inferred_group_category",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "today_request_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "total_request_count",
        "INTEGER",
    )?;
    add_column_if_missing(connection, "balance_snapshots", "today_consumption", "REAL")?;
    add_column_if_missing(connection, "balance_snapshots", "total_consumption", "REAL")?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "today_base_consumption",
        "REAL",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "total_base_consumption",
        "REAL",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "today_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "total_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "today_input_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "today_output_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "total_input_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "total_output_token_count",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "balance_snapshots",
        "account_concurrency_limit",
        "INTEGER",
    )?;

    Ok(())
}

fn migrate_secret_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS secrets (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            ciphertext TEXT NOT NULL,
            nonce TEXT NOT NULL,
            aad TEXT NOT NULL,
            masked_value TEXT NOT NULL,
            value_hash TEXT NOT NULL,
            encryption_version INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_secrets_owner_kind
            ON secrets(owner_id, kind);

        CREATE TABLE IF NOT EXISTS secret_migration_events (
            id TEXT PRIMARY KEY,
            owner_table TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            secret_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            error_message TEXT,
            created_at TEXT NOT NULL
        );
        ",
    )?;
    add_column_if_missing(connection, "station_keys", "api_key_secret_id", "TEXT")?;
    add_column_if_missing(connection, "stations", "api_key_secret_id", "TEXT")?;
    add_column_if_missing(
        connection,
        "station_credentials",
        "login_password_secret_id",
        "TEXT",
    )?;
    Ok(())
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    column_type: &str,
) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|existing| existing == column) {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}"),
            [],
        )?;
        return Ok(true);
    }

    Ok(false)
}

fn migrate_automatic_scheduler_schema(connection: &Connection) -> rusqlite::Result<()> {
    add_column_if_missing(
        connection,
        "station_keys",
        "max_concurrency",
        "INTEGER NOT NULL DEFAULT 3",
    )?;
    add_column_if_missing(connection, "station_keys", "load_factor", "INTEGER")?;
    let schedulable_column_added = add_column_if_missing(
        connection,
        "station_keys",
        "schedulable",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(connection, "station_keys", "manual_rate_multiplier", "REAL")?;
    add_column_if_missing(connection, "station_keys", "manual_rate_updated_at", "TEXT")?;
    if schedulable_column_added {
        // Existing disabled keys received the ADD COLUMN default of 1. Flip only
        // those rows during the one migration run that creates the column, so a
        // later user edit on an already-migrated database is preserved.
        connection.execute(
            "UPDATE station_keys
                SET schedulable = enabled
              WHERE enabled = 0
                AND schedulable = 1",
            [],
        )?;
    }
    Ok(())
}

fn validate_station_key_scheduler_values(
    max_concurrency: i64,
    load_factor: Option<i64>,
    manual_rate_multiplier: Option<f64>,
) -> Result<(), String> {
    if max_concurrency < 0 {
        return Err("max_concurrency must be >= 0".to_string());
    }
    if let Some(load_factor) = load_factor {
        if !(1..=10_000).contains(&load_factor) {
            return Err("load_factor must be between 1 and 10000".to_string());
        }
    }
    if let Some(manual_rate_multiplier) = manual_rate_multiplier {
        if !manual_rate_multiplier.is_finite() || manual_rate_multiplier < 0.0 {
            return Err("manual_rate_multiplier must be finite and >= 0".to_string());
        }
    }
    Ok(())
}

fn initialize_local_routing_order(connection: &Connection) -> rusqlite::Result<()> {
    let missing_count = connection.query_row(
        "SELECT COUNT(*) FROM station_keys WHERE routing_order IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if missing_count == 0 {
        return Ok(());
    }

    let existing_count = connection.query_row(
        "SELECT COUNT(*) FROM station_keys WHERE routing_order IS NOT NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let (start_order, query) = if existing_count == 0 {
        (
            0,
            "SELECT id
               FROM station_keys
              ORDER BY priority ASC, created_at ASC, id ASC",
        )
    } else {
        let next_order = connection.query_row(
            "SELECT COALESCE(MAX(routing_order), -1) + 1 FROM station_keys",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        (
            next_order,
            "SELECT id
               FROM station_keys
              WHERE routing_order IS NULL
              ORDER BY priority ASC, created_at ASC, id ASC",
        )
    };
    let mut statement = connection.prepare(query)?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    for (index, id) in ids.iter().enumerate() {
        connection.execute(
            "UPDATE station_keys
                SET routing_order = ?1
              WHERE id = ?2
                AND routing_order IS NULL",
            params![start_order + index as i64, id],
        )?;
    }

    Ok(())
}

fn secret_aad(scope: &str, owner_id: &str, kind: &str) -> String {
    format!("{scope}:{owner_id}:{kind}")
}

fn upsert_secret_in_connection(
    connection: &Connection,
    data_key: &[u8; 32],
    scope: &str,
    owner_id: &str,
    kind: &str,
    plaintext: &str,
) -> Result<String, String> {
    let existing_id: Option<String> = connection
        .query_row(
            "SELECT id FROM secrets WHERE owner_id = ?1 AND kind = ?2 ORDER BY updated_at DESC LIMIT 1",
            params![owner_id, kind],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取已有加密凭据失败: {error}"))?;
    let id = existing_id.unwrap_or_else(|| generate_id("secret"));
    let now = now_string();
    let aad = secret_aad(scope, owner_id, kind);
    let encrypted = encrypt_secret(data_key, plaintext, &aad)?;
    let masked = mask_sensitive_value(plaintext);

    connection
        .execute(
            "INSERT INTO secrets (
                id, scope, owner_id, kind, ciphertext, nonce, aad, masked_value,
                value_hash, encryption_version, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                ciphertext = excluded.ciphertext,
                nonce = excluded.nonce,
                aad = excluded.aad,
                masked_value = excluded.masked_value,
                value_hash = excluded.value_hash,
                encryption_version = excluded.encryption_version,
                updated_at = excluded.updated_at",
            params![
                id,
                scope,
                owner_id,
                kind,
                encrypted.ciphertext,
                encrypted.nonce,
                encrypted.aad,
                masked,
                encrypted.value_hash,
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存加密凭据失败: {error}"))?;

    Ok(id)
}

fn secret_payload_by_id(
    connection: &Connection,
    secret_id: &str,
) -> Result<EncryptedPayload, String> {
    connection
        .query_row(
            "SELECT ciphertext, nonce, aad, value_hash FROM secrets WHERE id = ?1",
            params![secret_id],
            |row| {
                Ok(EncryptedPayload {
                    ciphertext: row.get(0)?,
                    nonce: row.get(1)?,
                    aad: row.get(2)?,
                    value_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取加密凭据失败: {error}"))?
        .ok_or_else(|| "加密凭据不存在".to_string())
}

fn decrypt_secret_by_id(
    connection: &Connection,
    data_key: &[u8; 32],
    secret_id: &str,
) -> Result<String, String> {
    let payload = secret_payload_by_id(connection, secret_id)?;
    decrypt_secret(data_key, &payload)
}

fn resolve_station_key_api_key(
    connection: &Connection,
    data_key: &[u8; 32],
    station_key_id: &str,
) -> Result<String, String> {
    let (api_key, secret_id): (String, Option<String>) = connection
        .query_row(
            "SELECT api_key, api_key_secret_id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 凭据失败: {error}"))?
        .ok_or_else(|| "Station Key 不存在，无法读取凭据".to_string())?;

    if let Some(secret_id) = secret_id {
        return decrypt_secret_by_id(connection, data_key, &secret_id);
    }

    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        Err("Station Key 没有可用 API Key".to_string())
    } else {
        Ok(api_key)
    }
}

fn record_secret_migration_event(
    connection: &Connection,
    owner_table: &str,
    owner_id: &str,
    secret_kind: &str,
    status: &str,
    error_message: Option<String>,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO secret_migration_events (
                id, owner_table, owner_id, secret_kind, status, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                generate_id("secret_migration"),
                owner_table,
                owner_id,
                secret_kind,
                status,
                normalize_optional_string(error_message),
                now_string(),
            ],
        )
        .map_err(|error| format!("记录凭据迁移事件失败: {error}"))?;
    Ok(())
}

fn secret_migration_status_from_connection(
    connection: &Connection,
) -> Result<SecretMigrationReport, String> {
    let migrated_count = secret_migration_count(connection, "migrated")?;
    let failed_count = secret_migration_count(connection, "failed")?;
    let skipped_count = 0;
    let mut statement = connection
        .prepare(
            "SELECT owner_table, owner_id, secret_kind, error_message
               FROM secret_migration_events
              WHERE status = 'failed'
              ORDER BY created_at DESC
              LIMIT 20",
        )
        .map_err(|error| format!("读取凭据迁移状态失败: {error}"))?;
    let failures = statement
        .query_map([], |row| {
            let owner_table: String = row.get(0)?;
            let owner_id: String = row.get(1)?;
            let secret_kind: String = row.get(2)?;
            let error_message: Option<String> = row.get(3)?;
            Ok(format!(
                "{owner_table}/{owner_id}/{secret_kind}: {}",
                error_message.unwrap_or_else(|| "未知错误".to_string())
            ))
        })
        .map_err(|error| format!("查询凭据迁移失败列表失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析凭据迁移失败列表失败: {error}"))?;

    Ok(SecretMigrationReport {
        migrated_count,
        skipped_count,
        failed_count,
        failures,
    })
}

fn secret_migration_count(connection: &Connection, status: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM secret_migration_events WHERE status = ?1",
            params![status],
            |row| row.get(0),
        )
        .map_err(|error| format!("统计凭据迁移状态失败: {error}"))
}

fn run_secret_safety_scan_in_connection(
    connection: &Connection,
) -> Result<Vec<SecretScanFinding>, String> {
    let patterns = crate::services::secrets::audit::canary_patterns();
    let targets = [
        ("stations", "api_key"),
        ("station_keys", "api_key"),
        ("station_credentials", "login_password"),
        ("collector_snapshots", "summary_json"),
        ("collector_snapshots", "normalized_json"),
        ("collector_snapshots", "raw_json_redacted"),
        ("collector_snapshots", "error_message"),
        ("station_group_bindings", "raw_json_redacted"),
        ("group_rate_records", "raw_json_redacted"),
        ("collector_runs", "error_message"),
        ("request_logs", "error_message"),
        ("request_logs", "route_reason"),
        ("request_logs", "rejected_candidates_json"),
    ];
    let mut findings = Vec::new();

    for (table_name, column_name) in targets {
        let sql = format!("SELECT {column_name} FROM {table_name} WHERE {column_name} IS NOT NULL");
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("准备安全扫描失败: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .map_err(|error| format!("执行安全扫描失败: {error}"))?;

        for row in rows {
            let value = row
                .map_err(|error| format!("读取安全扫描结果失败: {error}"))?
                .unwrap_or_default();
            if patterns.iter().any(|pattern| value.contains(pattern)) {
                findings.push(crate::services::secrets::audit::finding(
                    table_name,
                    column_name,
                    &value,
                ));
            }
        }
    }

    Ok(findings)
}

fn migrate_plaintext_secrets_in_connection(
    connection: &Connection,
    data_key: &[u8; 32],
) -> Result<SecretMigrationReport, String> {
    let mut migrated_count = 0_i64;
    let mut skipped_count = 0_i64;
    let mut failed_count = 0_i64;
    let mut failures = Vec::new();

    migrate_plaintext_api_key_rows(
        connection,
        data_key,
        "station_keys",
        "station_key",
        "api_key",
        "api_key_secret_id",
        &mut migrated_count,
        &mut skipped_count,
        &mut failed_count,
        &mut failures,
    )?;
    migrate_plaintext_api_key_rows(
        connection,
        data_key,
        "stations",
        "station",
        "api_key",
        "api_key_secret_id",
        &mut migrated_count,
        &mut skipped_count,
        &mut failed_count,
        &mut failures,
    )?;
    migrate_plaintext_password_rows(
        connection,
        data_key,
        &mut migrated_count,
        &mut skipped_count,
        &mut failed_count,
        &mut failures,
    )?;

    Ok(SecretMigrationReport {
        migrated_count,
        skipped_count,
        failed_count,
        failures,
    })
}

#[allow(clippy::too_many_arguments)]
fn migrate_plaintext_api_key_rows(
    connection: &Connection,
    data_key: &[u8; 32],
    table: &str,
    scope: &str,
    plaintext_column: &str,
    secret_column: &str,
    migrated_count: &mut i64,
    skipped_count: &mut i64,
    failed_count: &mut i64,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let sql = format!("SELECT id, {plaintext_column}, {secret_column} FROM {table}");
    let rows = {
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("准备凭据迁移失败: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| format!("查询凭据迁移数据失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("解析凭据迁移数据失败: {error}"))?;
        rows
    };

    for (owner_id, plaintext, existing_secret_id) in rows {
        if plaintext.trim().is_empty() {
            *skipped_count += 1;
            continue;
        }
        if existing_secret_id.is_some() {
            *skipped_count += 1;
            continue;
        }

        match upsert_secret_in_connection(
            connection,
            data_key,
            scope,
            &owner_id,
            "api_key",
            plaintext.trim(),
        ) {
            Ok(secret_id) => {
                let update_sql = format!(
                    "UPDATE {table} SET {secret_column} = ?1, {plaintext_column} = '', updated_at = ?2 WHERE id = ?3"
                );
                connection
                    .execute(&update_sql, params![secret_id, now_string(), owner_id])
                    .map_err(|error| format!("清理明文凭据失败: {error}"))?;
                record_secret_migration_event(
                    connection, table, &owner_id, "api_key", "migrated", None,
                )?;
                *migrated_count += 1;
            }
            Err(error) => {
                let message = format!("{table}/{owner_id}: {error}");
                record_secret_migration_event(
                    connection,
                    table,
                    &owner_id,
                    "api_key",
                    "failed",
                    Some(error),
                )?;
                failures.push(message);
                *failed_count += 1;
            }
        }
    }

    Ok(())
}

fn migrate_plaintext_password_rows(
    connection: &Connection,
    data_key: &[u8; 32],
    migrated_count: &mut i64,
    skipped_count: &mut i64,
    failed_count: &mut i64,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let rows = {
        let mut statement = connection
            .prepare(
                "SELECT station_id, login_password, login_password_secret_id FROM station_credentials",
            )
            .map_err(|error| format!("准备登录密码迁移失败: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| format!("查询登录密码迁移数据失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("解析登录密码迁移数据失败: {error}"))?;
        rows
    };

    for (station_id, plaintext, existing_secret_id) in rows {
        let plaintext = plaintext.unwrap_or_default();
        if plaintext.trim().is_empty() {
            *skipped_count += 1;
            continue;
        }
        if existing_secret_id.is_some() {
            *skipped_count += 1;
            continue;
        }

        match upsert_secret_in_connection(
            connection,
            data_key,
            "station",
            &station_id,
            "login_password",
            plaintext.trim(),
        ) {
            Ok(secret_id) => {
                connection
                    .execute(
                        "UPDATE station_credentials
                            SET login_password_secret_id = ?1,
                                login_password = NULL,
                                updated_at = ?2
                          WHERE station_id = ?3",
                        params![secret_id, now_string(), station_id],
                    )
                    .map_err(|error| format!("清理明文登录密码失败: {error}"))?;
                record_secret_migration_event(
                    connection,
                    "station_credentials",
                    &station_id,
                    "login_password",
                    "migrated",
                    None,
                )?;
                *migrated_count += 1;
            }
            Err(error) => {
                let message = format!("station_credentials/{station_id}: {error}");
                record_secret_migration_event(
                    connection,
                    "station_credentials",
                    &station_id,
                    "login_password",
                    "failed",
                    Some(error),
                )?;
                failures.push(message);
                *failed_count += 1;
            }
        }
    }

    Ok(())
}

fn seed_default_settings(connection: &Connection) -> rusqlite::Result<()> {
    for (key, value) in DEFAULT_SETTINGS {
        connection.execute(
            "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value, now_string()],
        )?;
    }

    Ok(())
}

fn migrate_default_routing_strategy(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE settings
            SET value = 'cost_stable_first',
                updated_at = ?1
          WHERE key = 'default_routing_strategy'
            AND value = 'manual'",
        params![now_string()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct BuiltinModelBasePrice {
    id: &'static str,
    provider: &'static str,
    model: &'static str,
    input_price: f64,
    output_price: f64,
    source_url: &'static str,
    source_label: &'static str,
    note: &'static str,
}

const BUILTIN_MODEL_BASE_PRICE_CHECKED_AT: &str = "2026-07-12";
const BUILTIN_MODEL_BASE_PRICES: &[BuiltinModelBasePrice] = &[
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-3-7-sonnet-20250219",
        provider: "anthropic",
        model: "claude-3-7-sonnet-20250219",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-3-haiku-20240307",
        provider: "anthropic",
        model: "claude-3-haiku-20240307",
        input_price: 0.25,
        output_price: 1.25,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-3-opus-20240229",
        provider: "anthropic",
        model: "claude-3-opus-20240229",
        input_price: 15.0,
        output_price: 75.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-4-opus-20250514",
        provider: "anthropic",
        model: "claude-4-opus-20250514",
        input_price: 15.0,
        output_price: 75.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-4-sonnet-20250514",
        provider: "anthropic",
        model: "claude-4-sonnet-20250514",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-haiku-4-5",
        provider: "anthropic",
        model: "claude-haiku-4-5",
        input_price: 1.0,
        output_price: 5.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-haiku-4-5-20251001",
        provider: "anthropic",
        model: "claude-haiku-4-5-20251001",
        input_price: 1.0,
        output_price: 5.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-1",
        provider: "anthropic",
        model: "claude-opus-4-1",
        input_price: 15.0,
        output_price: 75.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-1-20250805",
        provider: "anthropic",
        model: "claude-opus-4-1-20250805",
        input_price: 15.0,
        output_price: 75.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-20250514",
        provider: "anthropic",
        model: "claude-opus-4-20250514",
        input_price: 15.0,
        output_price: 75.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-5",
        provider: "anthropic",
        model: "claude-opus-4-5",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-5-20251101",
        provider: "anthropic",
        model: "claude-opus-4-5-20251101",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-6",
        provider: "anthropic",
        model: "claude-opus-4-6",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-6-20260205",
        provider: "anthropic",
        model: "claude-opus-4-6-20260205",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-6-thinking",
        provider: "anthropic",
        model: "claude-opus-4-6-thinking",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-7",
        provider: "anthropic",
        model: "claude-opus-4-7",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-7-20260416",
        provider: "anthropic",
        model: "claude-opus-4-7-20260416",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-opus-4-8",
        provider: "anthropic",
        model: "claude-opus-4-8",
        input_price: 5.0,
        output_price: 25.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-sonnet-4-20250514",
        provider: "anthropic",
        model: "claude-sonnet-4-20250514",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-sonnet-4-5",
        provider: "anthropic",
        model: "claude-sonnet-4-5",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-sonnet-4-5-20250929",
        provider: "anthropic",
        model: "claude-sonnet-4-5-20250929",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-anthropic-claude-sonnet-4-6",
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-bedrock-claude-sonnet-4-5-20250929-v1-0",
        provider: "bedrock",
        model: "claude-sonnet-4-5-20250929-v1:0",
        input_price: 3.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-deepseek-deepseek-chat",
        provider: "deepseek",
        model: "deepseek-chat",
        input_price: 0.28,
        output_price: 0.42,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-deepseek-deepseek-reasoner",
        provider: "deepseek",
        model: "deepseek-reasoner",
        input_price: 0.28,
        output_price: 0.42,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-0-flash",
        provider: "google",
        model: "gemini-2.0-flash",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-0-flash-001",
        provider: "google",
        model: "gemini-2.0-flash-001",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-0-flash-exp-image-generation",
        provider: "google",
        model: "gemini-2.0-flash-exp-image-generation",
        input_price: 0.0,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-0-flash-lite",
        provider: "google",
        model: "gemini-2.0-flash-lite",
        input_price: 0.075,
        output_price: 0.3,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-0-flash-lite-001",
        provider: "google",
        model: "gemini-2.0-flash-lite-001",
        input_price: 0.075,
        output_price: 0.3,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-computer-use-preview-10-2025",
        provider: "google",
        model: "gemini-2.5-computer-use-preview-10-2025",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash",
        provider: "google",
        model: "gemini-2.5-flash",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-image",
        provider: "google",
        model: "gemini-2.5-flash-image",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-lite",
        provider: "google",
        model: "gemini-2.5-flash-lite",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-lite-preview-06-17",
        provider: "google",
        model: "gemini-2.5-flash-lite-preview-06-17",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-lite-preview-09-2025",
        provider: "google",
        model: "gemini-2.5-flash-lite-preview-09-2025",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-native-audio-latest",
        provider: "google",
        model: "gemini-2.5-flash-native-audio-latest",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-native-audio-preview-09-2025",
        provider: "google",
        model: "gemini-2.5-flash-native-audio-preview-09-2025",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-native-audio-preview-12-2025",
        provider: "google",
        model: "gemini-2.5-flash-native-audio-preview-12-2025",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-preview-09-2025",
        provider: "google",
        model: "gemini-2.5-flash-preview-09-2025",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-flash-preview-tts",
        provider: "google",
        model: "gemini-2.5-flash-preview-tts",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-pro",
        provider: "google",
        model: "gemini-2.5-pro",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-2-5-pro-preview-tts",
        provider: "google",
        model: "gemini-2.5-pro-preview-tts",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-flash",
        provider: "google",
        model: "gemini-3-flash",
        input_price: 0.5,
        output_price: 3.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-flash-preview",
        provider: "google",
        model: "gemini-3-flash-preview",
        input_price: 0.5,
        output_price: 3.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-pro-image",
        provider: "google",
        model: "gemini-3-pro-image",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-pro-image-preview",
        provider: "google",
        model: "gemini-3-pro-image-preview",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-pro-preview",
        provider: "google",
        model: "gemini-3-pro-preview",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-image",
        provider: "google",
        model: "gemini-3.1-flash-image",
        input_price: 0.5,
        output_price: 3.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-image-preview",
        provider: "google",
        model: "gemini-3.1-flash-image-preview",
        input_price: 0.5,
        output_price: 3.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-lite",
        provider: "google",
        model: "gemini-3.1-flash-lite",
        input_price: 0.25,
        output_price: 1.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-lite-image",
        provider: "google",
        model: "gemini-3.1-flash-lite-image",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-lite-preview",
        provider: "google",
        model: "gemini-3.1-flash-lite-preview",
        input_price: 0.25,
        output_price: 1.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-flash-live-preview",
        provider: "google",
        model: "gemini-3.1-flash-live-preview",
        input_price: 0.75,
        output_price: 4.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-pro-high",
        provider: "google",
        model: "gemini-3.1-pro-high",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-pro-low",
        provider: "google",
        model: "gemini-3.1-pro-low",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-pro-preview",
        provider: "google",
        model: "gemini-3.1-pro-preview",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-1-pro-preview-customtools",
        provider: "google",
        model: "gemini-3.1-pro-preview-customtools",
        input_price: 2.0,
        output_price: 12.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-3-5-flash",
        provider: "google",
        model: "gemini-3.5-flash",
        input_price: 1.5,
        output_price: 9.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-embedding-001",
        provider: "google",
        model: "gemini-embedding-001",
        input_price: 0.15,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-embedding-2",
        provider: "google",
        model: "gemini-embedding-2",
        input_price: 0.2,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-embedding-2-preview",
        provider: "google",
        model: "gemini-embedding-2-preview",
        input_price: 0.2,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-exp-1206",
        provider: "google",
        model: "gemini-exp-1206",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-flash-experimental",
        provider: "google",
        model: "gemini-flash-experimental",
        input_price: 0.0,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-flash-latest",
        provider: "google",
        model: "gemini-flash-latest",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-flash-lite-latest",
        provider: "google",
        model: "gemini-flash-lite-latest",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-live-2-5-flash-preview-native-audio-09-2025",
        provider: "google",
        model: "gemini-live-2.5-flash-preview-native-audio-09-2025",
        input_price: 0.3,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM realtime pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-pro-latest",
        provider: "google",
        model: "gemini-pro-latest",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-google-gemini-robotics-er-1-5-preview",
        provider: "google",
        model: "gemini-robotics-er-1.5-preview",
        input_price: 0.3,
        output_price: 2.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-codex-auto-review",
        provider: "openai",
        model: "codex-auto-review",
        input_price: 5.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-3-5-turbo",
        provider: "openai",
        model: "gpt-3.5-turbo",
        input_price: 0.5,
        output_price: 1.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-3-5-turbo-0125",
        provider: "openai",
        model: "gpt-3.5-turbo-0125",
        input_price: 0.5,
        output_price: 1.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-3-5-turbo-1106",
        provider: "openai",
        model: "gpt-3.5-turbo-1106",
        input_price: 1.0,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-3-5-turbo-16k",
        provider: "openai",
        model: "gpt-3.5-turbo-16k",
        input_price: 3.0,
        output_price: 4.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4",
        provider: "openai",
        model: "gpt-4",
        input_price: 30.0,
        output_price: 60.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-0125-preview",
        provider: "openai",
        model: "gpt-4-0125-preview",
        input_price: 10.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-0314",
        provider: "openai",
        model: "gpt-4-0314",
        input_price: 30.0,
        output_price: 60.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-0613",
        provider: "openai",
        model: "gpt-4-0613",
        input_price: 30.0,
        output_price: 60.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1106-preview",
        provider: "openai",
        model: "gpt-4-1106-preview",
        input_price: 10.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-turbo",
        provider: "openai",
        model: "gpt-4-turbo",
        input_price: 10.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-turbo-2024-04-09",
        provider: "openai",
        model: "gpt-4-turbo-2024-04-09",
        input_price: 10.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-turbo-preview",
        provider: "openai",
        model: "gpt-4-turbo-preview",
        input_price: 10.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1",
        provider: "openai",
        model: "gpt-4.1",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1-2025-04-14",
        provider: "openai",
        model: "gpt-4.1-2025-04-14",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1-mini",
        provider: "openai",
        model: "gpt-4.1-mini",
        input_price: 0.4,
        output_price: 1.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1-mini-2025-04-14",
        provider: "openai",
        model: "gpt-4.1-mini-2025-04-14",
        input_price: 0.4,
        output_price: 1.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1-nano",
        provider: "openai",
        model: "gpt-4.1-nano",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4-1-nano-2025-04-14",
        provider: "openai",
        model: "gpt-4.1-nano-2025-04-14",
        input_price: 0.1,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o",
        provider: "openai",
        model: "gpt-4o",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-2024-05-13",
        provider: "openai",
        model: "gpt-4o-2024-05-13",
        input_price: 5.0,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-2024-08-06",
        provider: "openai",
        model: "gpt-4o-2024-08-06",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-2024-11-20",
        provider: "openai",
        model: "gpt-4o-2024-11-20",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-audio-preview",
        provider: "openai",
        model: "gpt-4o-audio-preview",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-audio-preview-2024-12-17",
        provider: "openai",
        model: "gpt-4o-audio-preview-2024-12-17",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-audio-preview-2025-06-03",
        provider: "openai",
        model: "gpt-4o-audio-preview-2025-06-03",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini",
        provider: "openai",
        model: "gpt-4o-mini",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-2024-07-18",
        provider: "openai",
        model: "gpt-4o-mini-2024-07-18",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-audio-preview",
        provider: "openai",
        model: "gpt-4o-mini-audio-preview",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-audio-preview-2024-12-17",
        provider: "openai",
        model: "gpt-4o-mini-audio-preview-2024-12-17",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-realtime-preview",
        provider: "openai",
        model: "gpt-4o-mini-realtime-preview",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-realtime-preview-2024-12-17",
        provider: "openai",
        model: "gpt-4o-mini-realtime-preview-2024-12-17",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-search-preview",
        provider: "openai",
        model: "gpt-4o-mini-search-preview",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-search-preview-2025-03-11",
        provider: "openai",
        model: "gpt-4o-mini-search-preview-2025-03-11",
        input_price: 0.15,
        output_price: 0.6,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-transcribe",
        provider: "openai",
        model: "gpt-4o-mini-transcribe",
        input_price: 1.25,
        output_price: 5.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-transcribe-2025-03-20",
        provider: "openai",
        model: "gpt-4o-mini-transcribe-2025-03-20",
        input_price: 1.25,
        output_price: 5.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-transcribe-2025-12-15",
        provider: "openai",
        model: "gpt-4o-mini-transcribe-2025-12-15",
        input_price: 1.25,
        output_price: 5.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-tts",
        provider: "openai",
        model: "gpt-4o-mini-tts",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-tts-2025-03-20",
        provider: "openai",
        model: "gpt-4o-mini-tts-2025-03-20",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-mini-tts-2025-12-15",
        provider: "openai",
        model: "gpt-4o-mini-tts-2025-12-15",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-realtime-preview",
        provider: "openai",
        model: "gpt-4o-realtime-preview",
        input_price: 5.0,
        output_price: 20.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-realtime-preview-2024-12-17",
        provider: "openai",
        model: "gpt-4o-realtime-preview-2024-12-17",
        input_price: 5.0,
        output_price: 20.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-realtime-preview-2025-06-03",
        provider: "openai",
        model: "gpt-4o-realtime-preview-2025-06-03",
        input_price: 5.0,
        output_price: 20.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-search-preview",
        provider: "openai",
        model: "gpt-4o-search-preview",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-search-preview-2025-03-11",
        provider: "openai",
        model: "gpt-4o-search-preview-2025-03-11",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-transcribe",
        provider: "openai",
        model: "gpt-4o-transcribe",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-4o-transcribe-diarize",
        provider: "openai",
        model: "gpt-4o-transcribe-diarize",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5",
        provider: "openai",
        model: "gpt-5",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2025-08-07",
        provider: "openai",
        model: "gpt-5-2025-08-07",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-chat",
        provider: "openai",
        model: "gpt-5-chat",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-chat-latest",
        provider: "openai",
        model: "gpt-5-chat-latest",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-codex",
        provider: "openai",
        model: "gpt-5-codex",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-mini",
        provider: "openai",
        model: "gpt-5-mini",
        input_price: 0.25,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-mini-2025-08-07",
        provider: "openai",
        model: "gpt-5-mini-2025-08-07",
        input_price: 0.25,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-nano",
        provider: "openai",
        model: "gpt-5-nano",
        input_price: 0.05,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-nano-2025-08-07",
        provider: "openai",
        model: "gpt-5-nano-2025-08-07",
        input_price: 0.05,
        output_price: 0.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-pro",
        provider: "openai",
        model: "gpt-5-pro",
        input_price: 15.0,
        output_price: 120.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-pro-2025-10-06",
        provider: "openai",
        model: "gpt-5-pro-2025-10-06",
        input_price: 15.0,
        output_price: 120.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-search-api",
        provider: "openai",
        model: "gpt-5-search-api",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-search-api-2025-10-14",
        provider: "openai",
        model: "gpt-5-search-api-2025-10-14",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1",
        provider: "openai",
        model: "gpt-5.1",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1-2025-11-13",
        provider: "openai",
        model: "gpt-5.1-2025-11-13",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1-chat-latest",
        provider: "openai",
        model: "gpt-5.1-chat-latest",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1-codex",
        provider: "openai",
        model: "gpt-5.1-codex",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1-codex-max",
        provider: "openai",
        model: "gpt-5.1-codex-max",
        input_price: 1.25,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-1-codex-mini",
        provider: "openai",
        model: "gpt-5.1-codex-mini",
        input_price: 0.25,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2",
        provider: "openai",
        model: "gpt-5.2",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2-2025-12-11",
        provider: "openai",
        model: "gpt-5.2-2025-12-11",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2-chat-latest",
        provider: "openai",
        model: "gpt-5.2-chat-latest",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2-codex",
        provider: "openai",
        model: "gpt-5.2-codex",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2-pro",
        provider: "openai",
        model: "gpt-5.2-pro",
        input_price: 21.0,
        output_price: 168.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-2-pro-2025-12-11",
        provider: "openai",
        model: "gpt-5.2-pro-2025-12-11",
        input_price: 21.0,
        output_price: 168.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-3-chat-latest",
        provider: "openai",
        model: "gpt-5.3-chat-latest",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-3-codex",
        provider: "openai",
        model: "gpt-5.3-codex",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-3-codex-spark",
        provider: "openai",
        model: "gpt-5.3-codex-spark",
        input_price: 1.75,
        output_price: 14.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4",
        provider: "openai",
        model: "gpt-5.4",
        input_price: 2.5,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-2026-03-05",
        provider: "openai",
        model: "gpt-5.4-2026-03-05",
        input_price: 2.5,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-mini",
        provider: "openai",
        model: "gpt-5.4-mini",
        input_price: 0.75,
        output_price: 4.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-mini-2026-03-17",
        provider: "openai",
        model: "gpt-5.4-mini-2026-03-17",
        input_price: 0.75,
        output_price: 4.5,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-nano",
        provider: "openai",
        model: "gpt-5.4-nano",
        input_price: 0.2,
        output_price: 1.25,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-nano-2026-03-17",
        provider: "openai",
        model: "gpt-5.4-nano-2026-03-17",
        input_price: 0.2,
        output_price: 1.25,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-pro",
        provider: "openai",
        model: "gpt-5.4-pro",
        input_price: 30.0,
        output_price: 180.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-4-pro-2026-03-05",
        provider: "openai",
        model: "gpt-5.4-pro-2026-03-05",
        input_price: 30.0,
        output_price: 180.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-5",
        provider: "openai",
        model: "gpt-5.5",
        input_price: 5.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-5-2026-04-23",
        provider: "openai",
        model: "gpt-5.5-2026-04-23",
        input_price: 5.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-5-pro",
        provider: "openai",
        model: "gpt-5.5-pro",
        input_price: 30.0,
        output_price: 180.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-5-pro-2026-04-23",
        provider: "openai",
        model: "gpt-5.5-pro-2026-04-23",
        input_price: 30.0,
        output_price: 180.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-6-luna",
        provider: "openai",
        model: "gpt-5.6-luna",
        input_price: 1.0,
        output_price: 6.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-6-sol",
        provider: "openai",
        model: "gpt-5.6-sol",
        input_price: 5.0,
        output_price: 30.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-5-6-terra",
        provider: "openai",
        model: "gpt-5.6-terra",
        input_price: 2.5,
        output_price: 15.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio",
        provider: "openai",
        model: "gpt-audio",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio-1-5",
        provider: "openai",
        model: "gpt-audio-1.5",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio-2025-08-28",
        provider: "openai",
        model: "gpt-audio-2025-08-28",
        input_price: 2.5,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio-mini",
        provider: "openai",
        model: "gpt-audio-mini",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio-mini-2025-10-06",
        provider: "openai",
        model: "gpt-audio-mini-2025-10-06",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-audio-mini-2025-12-15",
        provider: "openai",
        model: "gpt-audio-mini-2025-12-15",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-1",
        provider: "openai",
        model: "gpt-image-1",
        input_price: 5.0,
        output_price: 40.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image-generation pricing: input_cost_per_token and output_cost_per_image_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-1-mini",
        provider: "openai",
        model: "gpt-image-1-mini",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image-generation pricing: input_cost_per_token and output_cost_per_image_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-1-5",
        provider: "openai",
        model: "gpt-image-1.5",
        input_price: 5.0,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-1-5-2025-12-16",
        provider: "openai",
        model: "gpt-image-1.5-2025-12-16",
        input_price: 5.0,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-2",
        provider: "openai",
        model: "gpt-image-2",
        input_price: 5.0,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-image-2-2026-04-21",
        provider: "openai",
        model: "gpt-image-2-2026-04-21",
        input_price: 5.0,
        output_price: 10.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime",
        provider: "openai",
        model: "gpt-realtime",
        input_price: 4.0,
        output_price: 16.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-1-5",
        provider: "openai",
        model: "gpt-realtime-1.5",
        input_price: 4.0,
        output_price: 16.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-2",
        provider: "openai",
        model: "gpt-realtime-2",
        input_price: 4.0,
        output_price: 16.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-2025-08-28",
        provider: "openai",
        model: "gpt-realtime-2025-08-28",
        input_price: 4.0,
        output_price: 16.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-mini",
        provider: "openai",
        model: "gpt-realtime-mini",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-mini-2025-10-06",
        provider: "openai",
        model: "gpt-realtime-mini-2025-10-06",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-gpt-realtime-mini-2025-12-15",
        provider: "openai",
        model: "gpt-realtime-mini-2025-12-15",
        input_price: 0.6,
        output_price: 2.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o1-2024-12-17",
        provider: "openai",
        model: "o1-2024-12-17",
        input_price: 15.0,
        output_price: 60.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o1-pro",
        provider: "openai",
        model: "o1-pro",
        input_price: 150.0,
        output_price: 600.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o1-pro-2025-03-19",
        provider: "openai",
        model: "o1-pro-2025-03-19",
        input_price: 150.0,
        output_price: 600.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3",
        provider: "openai",
        model: "o3",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-2025-04-16",
        provider: "openai",
        model: "o3-2025-04-16",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-deep-research",
        provider: "openai",
        model: "o3-deep-research",
        input_price: 10.0,
        output_price: 40.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-deep-research-2025-06-26",
        provider: "openai",
        model: "o3-deep-research-2025-06-26",
        input_price: 10.0,
        output_price: 40.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-mini",
        provider: "openai",
        model: "o3-mini",
        input_price: 1.1,
        output_price: 4.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-mini-2025-01-31",
        provider: "openai",
        model: "o3-mini-2025-01-31",
        input_price: 1.1,
        output_price: 4.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-pro",
        provider: "openai",
        model: "o3-pro",
        input_price: 20.0,
        output_price: 80.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o3-pro-2025-06-10",
        provider: "openai",
        model: "o3-pro-2025-06-10",
        input_price: 20.0,
        output_price: 80.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o4-mini",
        provider: "openai",
        model: "o4-mini",
        input_price: 1.1,
        output_price: 4.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o4-mini-2025-04-16",
        provider: "openai",
        model: "o4-mini-2025-04-16",
        input_price: 1.1,
        output_price: 4.4,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o4-mini-deep-research",
        provider: "openai",
        model: "o4-mini-deep-research",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-openai-o4-mini-deep-research-2025-06-26",
        provider: "openai",
        model: "o4-mini-deep-research-2025-06-26",
        input_price: 2.0,
        output_price: 8.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-text-completion-openai-gpt-3-5-turbo-instruct",
        provider: "text-completion-openai",
        model: "gpt-3.5-turbo-instruct",
        input_price: 1.5,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM completion pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-text-completion-openai-gpt-3-5-turbo-instruct-0914",
        provider: "text-completion-openai",
        model: "gpt-3.5-turbo-instruct-0914",
        input_price: 1.5,
        output_price: 2.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM completion pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
    BuiltinModelBasePrice {
        id: "builtin-volcengine-deepseek-v3-2-251201",
        provider: "volcengine",
        model: "deepseek-v3-2-251201",
        input_price: 0.0,
        output_price: 0.0,
        source_url: "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json",
        source_label: "Sub2API model pricing catalog",
        note: "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M.",
    },
];

fn seed_builtin_model_base_prices(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute("DELETE FROM model_base_prices WHERE built_in = 1", [])?;
    let now = now_string();
    for price in BUILTIN_MODEL_BASE_PRICES {
        connection.execute(
            "INSERT INTO model_base_prices (
                id, provider, model, input_price, output_price, currency, unit,
                source_url, source_label, source_checked_at, enabled, built_in, note,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'USD', 'M', ?6, ?7, ?8, 1, 1, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                currency = excluded.currency,
                unit = excluded.unit,
                source_url = excluded.source_url,
                source_label = excluded.source_label,
                source_checked_at = excluded.source_checked_at,
                built_in = 1,
                note = excluded.note,
                updated_at = excluded.updated_at",
            params![
                price.id,
                price.provider,
                price.model,
                price.input_price,
                price.output_price,
                price.source_url,
                price.source_label,
                BUILTIN_MODEL_BASE_PRICE_CHECKED_AT,
                price.note,
                now,
                now,
            ],
        )?;
    }

    Ok(())
}

fn seed_builtin_channel_monitor_templates_in_connection(
    connection: &Connection,
) -> rusqlite::Result<()> {
    let templates = [
        (
            "builtin-openai-chat-default",
            "OpenAI Chat Default",
            "chat_completions",
            "/v1/chat/completions",
            json!({
                "model": "{{model}}",
                "messages": [
                    { "role": "user", "content": "{{challenge}}" }
                ],
                "stream": "{{stream}}",
                "temperature": 0
            }),
        ),
        (
            "builtin-openai-chat-low-token",
            "OpenAI Chat Low Token",
            "chat_completions",
            "/v1/chat/completions",
            json!({
                "model": "{{model}}",
                "messages": [
                    { "role": "user", "content": "{{challenge}}" }
                ],
                "max_tokens": 1,
                "stream": "{{stream}}",
                "temperature": 0
            }),
        ),
        (
            "builtin-openai-responses-default",
            "OpenAI Responses Default",
            "responses",
            "/v1/responses",
            json!({
                "model": "{{model}}",
                "instructions": "Reply with OK only.",
                "input": "{{challenge}}",
                "store": false,
                "stream": "{{stream}}",
                "reasoning": { "effort": "minimal" },
                "temperature": 0
            }),
        ),
        (
            "builtin-openai-responses-low-token",
            "OpenAI Responses Low Token",
            "responses",
            "/v1/responses",
            json!({
                "model": "{{model}}",
                "instructions": "Reply with OK only.",
                "input": "{{challenge}}",
                "max_output_tokens": 1,
                "store": false,
                "stream": "{{stream}}",
                "reasoning": { "effort": "minimal" },
                "temperature": 0
            }),
        ),
    ];

    for (id, name, endpoint_kind, path, body) in templates {
        connection.execute(
            "INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'POST', ?4, ?5, 1, 1, NULL, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                endpoint_kind = excluded.endpoint_kind,
                method = excluded.method,
                path = excluded.path,
                request_body_json = excluded.request_body_json,
                enabled = 1,
                built_in = 1,
                updated_at = excluded.updated_at",
            params![
                id,
                name,
                endpoint_kind,
                path,
                body.to_string(),
                now_string(),
                now_string()
            ],
        )?;
    }

    Ok(())
}

fn row_to_channel_monitor_template(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ChannelMonitorRequestTemplate> {
    Ok(ChannelMonitorRequestTemplate {
        id: row.get(0)?,
        name: row.get(1)?,
        endpoint_kind: row.get(2)?,
        method: row.get(3)?,
        path: row.get(4)?,
        request_body_json: row.get(5)?,
        enabled: i64_to_bool(row.get(6)?),
        built_in: i64_to_bool(row.get(7)?),
        note: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn list_channel_monitor_templates_from_connection(
    connection: &Connection,
) -> Result<Vec<ChannelMonitorRequestTemplate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, endpoint_kind, method, path, request_body_json,
                    enabled, built_in, note, created_at, updated_at
               FROM channel_monitor_request_templates
              ORDER BY built_in DESC, name ASC, created_at ASC",
        )
        .map_err(|error| format!("读取通道监控模板失败: {error}"))?;

    let templates = statement
        .query_map([], row_to_channel_monitor_template)
        .map_err(|error| format!("查询通道监控模板失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析通道监控模板失败: {error}"))?;
    Ok(templates)
}

fn channel_monitor_template_by_id(
    connection: &Connection,
    id: &str,
) -> Result<ChannelMonitorRequestTemplate, String> {
    connection
        .query_row(
            "SELECT id, name, endpoint_kind, method, path, request_body_json,
                    enabled, built_in, note, created_at, updated_at
               FROM channel_monitor_request_templates
              WHERE id = ?1",
            params![id],
            row_to_channel_monitor_template,
        )
        .optional()
        .map_err(|error| format!("读取通道监控模板失败: {error}"))?
        .ok_or_else(|| "Channel monitor template does not exist".to_string())
}

fn validate_channel_monitor_template_fields(
    name: &str,
    endpoint_kind: &str,
    method: &str,
    path: &str,
    request_body_json: &str,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Channel monitor template name cannot be empty".to_string());
    }
    if endpoint_kind.trim().is_empty() {
        return Err("Channel monitor endpoint kind cannot be empty".to_string());
    }
    if method.trim().is_empty() {
        return Err("Channel monitor method cannot be empty".to_string());
    }
    let path = path.trim();
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains("://")
        || path
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(
            "Channel monitor path must be a same-origin path starting with exactly one /"
                .to_string(),
        );
    }
    serde_json::from_str::<Value>(request_body_json)
        .map_err(|error| format!("Channel monitor request body must be valid JSON: {error}"))?;
    Ok(())
}

fn create_channel_monitor_template_in_connection(
    connection: &Connection,
    input: CreateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    validate_channel_monitor_template_fields(
        &input.name,
        &input.endpoint_kind,
        &input.method,
        &input.path,
        &input.request_body_json,
    )?;
    let id = generate_id("channel_monitor_template");
    let now = now_string();

    connection
        .execute(
            "INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10)",
            params![
                id,
                input.name.trim(),
                input.endpoint_kind.trim(),
                input.method.trim().to_uppercase(),
                input.path.trim(),
                input.request_body_json,
                bool_to_i64(input.enabled),
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("创建通道监控模板失败: {error}"))?;

    channel_monitor_template_by_id(connection, &id)
}

fn update_channel_monitor_template_in_connection(
    connection: &Connection,
    input: UpdateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    let existing = channel_monitor_template_by_id(connection, &input.id)?;
    if existing.built_in {
        return Err("Built-in channel monitor templates cannot be updated".to_string());
    }
    validate_channel_monitor_template_fields(
        &input.name,
        &input.endpoint_kind,
        &input.method,
        &input.path,
        &input.request_body_json,
    )?;

    connection
        .execute(
            "UPDATE channel_monitor_request_templates
                SET name = ?1,
                    endpoint_kind = ?2,
                    method = ?3,
                    path = ?4,
                    request_body_json = ?5,
                    enabled = ?6,
                    note = ?7,
                    updated_at = ?8
              WHERE id = ?9",
            params![
                input.name.trim(),
                input.endpoint_kind.trim(),
                input.method.trim().to_uppercase(),
                input.path.trim(),
                input.request_body_json,
                bool_to_i64(input.enabled),
                normalize_optional_string(input.note),
                now_string(),
                input.id,
            ],
        )
        .map_err(|error| format!("更新通道监控模板失败: {error}"))?;

    channel_monitor_template_by_id(connection, &input.id)
}

fn duplicate_channel_monitor_template_in_connection(
    connection: &Connection,
    id: &str,
) -> Result<ChannelMonitorRequestTemplate, String> {
    let source = channel_monitor_template_by_id(connection, id)?;
    let copy_id = generate_id("channel_monitor_template");
    let now = now_string();
    let copy_name = format!("{} Copy", source.name);

    connection
        .execute(
            "INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10)",
            params![
                copy_id,
                copy_name,
                source.endpoint_kind,
                source.method,
                source.path,
                source.request_body_json,
                bool_to_i64(source.enabled),
                source.note,
                now,
                now,
            ],
        )
        .map_err(|error| format!("复制通道监控模板失败: {error}"))?;

    channel_monitor_template_by_id(connection, &copy_id)
}

fn delete_channel_monitor_template_in_connection(
    connection: &Connection,
    id: &str,
) -> Result<(), String> {
    let template = channel_monitor_template_by_id(connection, id)?;
    if template.built_in {
        return Err("Built-in channel monitor templates cannot be deleted".to_string());
    }
    let references: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM channel_monitors WHERE template_id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|error| format!("检查通道监控模板引用失败: {error}"))?;
    if references > 0 {
        return Err("Channel monitor template is referenced by channel monitors".to_string());
    }

    connection
        .execute(
            "DELETE FROM channel_monitor_request_templates WHERE id = ?1",
            params![id],
        )
        .map_err(|error| format!("删除通道监控模板失败: {error}"))?;
    Ok(())
}

fn row_to_channel_monitor(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelMonitor> {
    let fallback_models_json: String = row.get(12)?;
    let fallback_models =
        serde_json::from_str::<Vec<String>>(&fallback_models_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(12, Type::Text, Box::new(error))
        })?;

    Ok(ChannelMonitor {
        id: row.get(0)?,
        name: row.get(1)?,
        target_type: row.get(2)?,
        station_id: row.get(3)?,
        station_key_id: row.get(4)?,
        template_id: row.get(5)?,
        enabled: i64_to_bool(row.get(6)?),
        interval_seconds: row.get(7)?,
        jitter_seconds: row.get(8)?,
        timeout_seconds: row.get(9)?,
        max_concurrency: row.get(10)?,
        consecutive_failure_threshold: row.get(11)?,
        fallback_models,
        note: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn list_channel_monitors_from_connection(
    connection: &Connection,
) -> Result<Vec<ChannelMonitor>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, target_type, station_id, station_key_id, template_id,
                    enabled, interval_seconds, jitter_seconds, timeout_seconds,
                    max_concurrency, consecutive_failure_threshold, fallback_models_json,
                    note, created_at, updated_at
               FROM channel_monitors
              ORDER BY enabled DESC, station_id ASC, created_at ASC",
        )
        .map_err(|error| format!("读取通道监控失败: {error}"))?;

    let monitors = statement
        .query_map([], row_to_channel_monitor)
        .map_err(|error| format!("查询通道监控失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析通道监控失败: {error}"))?;
    Ok(monitors)
}

fn channel_monitor_by_id(connection: &Connection, id: &str) -> Result<ChannelMonitor, String> {
    connection
        .query_row(
            "SELECT id, name, target_type, station_id, station_key_id, template_id,
                    enabled, interval_seconds, jitter_seconds, timeout_seconds,
                    max_concurrency, consecutive_failure_threshold, fallback_models_json,
                    note, created_at, updated_at
               FROM channel_monitors
              WHERE id = ?1",
            params![id],
            row_to_channel_monitor,
        )
        .optional()
        .map_err(|error| format!("读取通道监控失败: {error}"))?
        .ok_or_else(|| "Channel monitor does not exist".to_string())
}

fn validate_enabled_channel_monitor_template(
    connection: &Connection,
    template_id: &str,
) -> Result<(), String> {
    let template = channel_monitor_template_by_id(connection, template_id)?;
    if !template.enabled {
        return Err("Channel monitor template is disabled".to_string());
    }
    Ok(())
}

fn validate_station_key_belongs_to_station(
    connection: &Connection,
    station_id: &str,
    station_key_id: &str,
) -> Result<(), String> {
    // SQLite CHECK constraints cannot query station_keys, so station/key ownership
    // remains guarded here before monitor writes.
    let owner_station_id: Option<String> = connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?;

    match owner_station_id {
        Some(owner_station_id) if owner_station_id == station_id => Ok(()),
        Some(_) => Err("Station key does not belong to station".to_string()),
        None => Err("Station key does not exist".to_string()),
    }
}

fn validate_channel_monitor_values(
    connection: &Connection,
    name: &str,
    target_type: &str,
    station_id: &str,
    station_key_id: Option<&str>,
    template_id: &str,
    interval_seconds: i64,
    jitter_seconds: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    consecutive_failure_threshold: i64,
) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("Channel monitor name cannot be empty".to_string());
    }
    let target_type = match target_type.trim() {
        "station" => "station".to_string(),
        "station_key" => "station_key".to_string(),
        _ => return Err("Channel monitor target_type must be station_key or station".to_string()),
    };
    validate_station_exists(connection, station_id)?;
    match target_type.as_str() {
        "station" => {
            if station_key_id.is_some() {
                return Err("Station-wide channel monitor cannot have station_key_id".to_string());
            }
        }
        "station_key" => {
            let Some(station_key_id) = station_key_id else {
                return Err("Key channel monitor requires station_key_id".to_string());
            };
            validate_station_key_belongs_to_station(connection, station_id, station_key_id)?;
        }
        _ => unreachable!(),
    }
    validate_enabled_channel_monitor_template(connection, template_id)?;

    if !(15..=3600).contains(&interval_seconds) {
        return Err("Channel monitor interval_seconds must be between 15 and 3600".to_string());
    }
    if !(0..=600).contains(&jitter_seconds) {
        return Err("Channel monitor jitter_seconds must be between 0 and 600".to_string());
    }
    if interval_seconds - jitter_seconds < 15 {
        return Err(
            "Channel monitor interval_seconds minus jitter_seconds must be at least 15".to_string(),
        );
    }
    if !(5..=120).contains(&timeout_seconds) {
        return Err("Channel monitor timeout_seconds must be between 5 and 120".to_string());
    }
    if !(1..=16).contains(&max_concurrency) {
        return Err("Channel monitor max_concurrency must be between 1 and 16".to_string());
    }
    if !(1..=20).contains(&consecutive_failure_threshold) {
        return Err(
            "Channel monitor consecutive_failure_threshold must be between 1 and 20".to_string(),
        );
    }

    Ok(target_type)
}

fn validate_channel_monitor_run_input(input: &CreateChannelMonitorRunInput) -> Result<(), String> {
    match input.status.trim() {
        "success" | "warning" | "failed" | "skipped" => {}
        _ => {
            return Err(
                "Channel monitor run status must be success, warning, failed, or skipped"
                    .to_string(),
            )
        }
    }
    let started_at = parse_channel_monitor_run_time(&input.started_at, "started_at")?;
    if let Some(finished_at) = input.finished_at.as_deref() {
        let finished_at = parse_channel_monitor_run_time(finished_at, "finished_at")?;
        if finished_at < started_at {
            return Err(
                "Channel monitor run finished_at cannot be earlier than started_at".to_string(),
            );
        }
    }
    if input.duration_ms.is_some_and(|value| value < 0) {
        return Err("Channel monitor run duration_ms must be non-negative".to_string());
    }
    if input.latency_ms.is_some_and(|value| value < 0) {
        return Err("Channel monitor run latency_ms must be non-negative".to_string());
    }
    if input
        .http_status
        .is_some_and(|value| !(100..=599).contains(&value))
    {
        return Err("Channel monitor run status_code must be between 100 and 599".to_string());
    }
    Ok(())
}

fn parse_channel_monitor_run_time(value: &str, field_name: &str) -> Result<i64, String> {
    let timestamp = value.trim().parse::<i64>().map_err(|_| {
        format!("Channel monitor run {field_name} must be a positive millisecond epoch")
    })?;
    if timestamp <= 0 {
        return Err(format!(
            "Channel monitor run {field_name} must be a positive millisecond epoch"
        ));
    }
    Ok(timestamp)
}

fn parse_optional_millisecond_timestamp(value: Option<&str>) -> Option<i64> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<i64>().ok())
}

fn create_channel_monitor_in_connection(
    connection: &Connection,
    input: CreateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    let target_type = validate_channel_monitor_values(
        connection,
        &input.name,
        &input.target_type,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    let id = generate_id("channel_monitor");
    let now = now_string();
    let fallback_models_json = serde_json::to_string(&input.fallback_models)
        .map_err(|error| format!("序列化 fallback models 失败: {error}"))?;

    connection
        .execute(
            "INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                id,
                input.name.trim(),
                target_type,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                input.template_id,
                bool_to_i64(input.enabled),
                input.interval_seconds,
                input.jitter_seconds,
                input.timeout_seconds,
                input.max_concurrency,
                input.consecutive_failure_threshold,
                fallback_models_json,
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("创建通道监控失败: {error}"))?;

    channel_monitor_by_id(connection, &id)
}

fn update_channel_monitor_in_connection(
    connection: &Connection,
    input: UpdateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    channel_monitor_by_id(connection, &input.id)?;
    let target_type = validate_channel_monitor_values(
        connection,
        &input.name,
        &input.target_type,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    let fallback_models_json = serde_json::to_string(&input.fallback_models)
        .map_err(|error| format!("序列化 fallback models 失败: {error}"))?;

    connection
        .execute(
            "UPDATE channel_monitors
                SET name = ?1,
                    target_type = ?2,
                    station_id = ?3,
                    station_key_id = ?4,
                    template_id = ?5,
                    enabled = ?6,
                    interval_seconds = ?7,
                    jitter_seconds = ?8,
                    timeout_seconds = ?9,
                    max_concurrency = ?10,
                    consecutive_failure_threshold = ?11,
                    fallback_models_json = ?12,
                    note = ?13,
                    updated_at = ?14
              WHERE id = ?15",
            params![
                input.name.trim(),
                target_type,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                input.template_id,
                bool_to_i64(input.enabled),
                input.interval_seconds,
                input.jitter_seconds,
                input.timeout_seconds,
                input.max_concurrency,
                input.consecutive_failure_threshold,
                fallback_models_json,
                normalize_optional_string(input.note),
                now_string(),
                input.id,
            ],
        )
        .map_err(|error| format!("更新通道监控失败: {error}"))?;

    channel_monitor_by_id(connection, &input.id)
}

fn delete_channel_monitor_in_connection(connection: &Connection, id: &str) -> Result<(), String> {
    let deleted = connection
        .execute("DELETE FROM channel_monitors WHERE id = ?1", params![id])
        .map_err(|error| format!("删除通道监控失败: {error}"))?;
    if deleted == 0 {
        return Err("Channel monitor does not exist".to_string());
    }
    Ok(())
}

fn row_to_channel_monitor_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelMonitorRun> {
    Ok(ChannelMonitorRun {
        id: row.get(0)?,
        monitor_id: row.get(1)?,
        template_id: row.get(2)?,
        station_id: row.get(3)?,
        station_key_id: row.get(4)?,
        status: row.get(5)?,
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        duration_ms: row.get(8)?,
        http_status: row.get(9)?,
        latency_ms: row.get(10)?,
        response_model: row.get(11)?,
        fallback_model: row.get(12)?,
        error_message: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn list_channel_monitor_runs_from_connection(
    connection: &Connection,
    monitor_id: &str,
) -> Result<Vec<ChannelMonitorRun>, String> {
    channel_monitor_by_id(connection, monitor_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, monitor_id, template_id, station_id, station_key_id, status,
                    started_at, finished_at, duration_ms, http_status, latency_ms,
                    response_model, fallback_model, error_message, created_at
               FROM channel_monitor_runs
              WHERE monitor_id = ?1
              ORDER BY created_at DESC",
        )
        .map_err(|error| format!("读取通道监控运行记录失败: {error}"))?;

    let runs = statement
        .query_map(params![monitor_id], row_to_channel_monitor_run)
        .map_err(|error| format!("查询通道监控运行记录失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析通道监控运行记录失败: {error}"))?;
    Ok(runs)
}

const DEFAULT_CHANNEL_MONITOR_SUMMARY_RUN_LIMIT: usize = 60;
const MAX_CHANNEL_MONITOR_SUMMARY_RUN_LIMIT: usize = 10_080;

fn normalize_channel_monitor_summary_run_limit(run_limit: Option<usize>) -> usize {
    run_limit
        .unwrap_or(DEFAULT_CHANNEL_MONITOR_SUMMARY_RUN_LIMIT)
        .clamp(1, MAX_CHANNEL_MONITOR_SUMMARY_RUN_LIMIT)
}

fn list_channel_monitor_runs_for_summary_from_connection(
    connection: &Connection,
    monitor_id: &str,
    run_since: Option<&str>,
    run_limit: Option<usize>,
) -> Result<Vec<ChannelMonitorRun>, String> {
    channel_monitor_by_id(connection, monitor_id)?;
    let limit = normalize_channel_monitor_summary_run_limit(run_limit) as i64;
    if let Some(run_since) = run_since {
        let run_since = parse_channel_monitor_run_time(run_since, "run_since")?;
        let mut statement = connection
            .prepare(
                "SELECT id, monitor_id, template_id, station_id, station_key_id, status,
                        started_at, finished_at, duration_ms, http_status, latency_ms,
                        response_model, fallback_model, error_message, created_at
                   FROM channel_monitor_runs
                  WHERE monitor_id = ?1
                    AND CAST(started_at AS INTEGER) >= ?2
                  ORDER BY created_at DESC
                  LIMIT ?3",
            )
            .map_err(|error| format!("读取通道监控运行记录失败: {error}"))?;

        let runs = statement
            .query_map(
                params![monitor_id, run_since, limit],
                row_to_channel_monitor_run,
            )
            .map_err(|error| format!("查询通道监控运行记录失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("解析通道监控运行记录失败: {error}"))?;
        return Ok(runs);
    }

    let mut statement = connection
        .prepare(
            "SELECT id, monitor_id, template_id, station_id, station_key_id, status,
                    started_at, finished_at, duration_ms, http_status, latency_ms,
                    response_model, fallback_model, error_message, created_at
               FROM channel_monitor_runs
              WHERE monitor_id = ?1
              ORDER BY created_at DESC
              LIMIT ?2",
        )
        .map_err(|error| format!("读取通道监控运行记录失败: {error}"))?;

    let runs = statement
        .query_map(params![monitor_id, limit], row_to_channel_monitor_run)
        .map_err(|error| format!("查询通道监控运行记录失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析通道监控运行记录失败: {error}"))?;
    Ok(runs)
}

fn channel_status_window_facts_from_connection(
    connection: &Connection,
    monitor_id: &str,
    since_ms: Option<i64>,
    timeline_limit: usize,
) -> Result<ChannelStatusWindowFacts, String> {
    channel_monitor_by_id(connection, monitor_id)?;
    let timeline_limit = timeline_limit.clamp(1, CHANNEL_STATUS_TIMELINE_LIMIT) as i64;

    let aggregate_sql = if since_ms.is_some() {
        "SELECT
            COUNT(*) AS total_count,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failure_count,
            SUM(CASE WHEN status IN ('warning', 'skipped') THEN 1 ELSE 0 END) AS warning_count,
            CAST(AVG(COALESCE(latency_ms, duration_ms)) AS INTEGER) AS avg_latency_ms
         FROM channel_monitor_runs
         WHERE monitor_id = ?1 AND CAST(started_at AS INTEGER) >= ?2"
    } else {
        "WITH latest_runs AS (
            SELECT status, latency_ms, duration_ms
              FROM channel_monitor_runs
             WHERE monitor_id = ?1
             ORDER BY CAST(started_at AS INTEGER) DESC
             LIMIT ?2
         )
         SELECT
            COUNT(*) AS total_count,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failure_count,
            SUM(CASE WHEN status IN ('warning', 'skipped') THEN 1 ELSE 0 END) AS warning_count,
            CAST(AVG(COALESCE(latency_ms, duration_ms)) AS INTEGER) AS avg_latency_ms
         FROM latest_runs"
    };

    let read_aggregate = |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            row.get::<_, Option<i64>>(4)?,
        ))
    };
    let (total_count, success_count, failure_count, warning_count, avg_latency_ms) =
        if let Some(since_ms) = since_ms {
            connection.query_row(aggregate_sql, params![monitor_id, since_ms], read_aggregate)
        } else {
            connection.query_row(
                aggregate_sql,
                params![monitor_id, timeline_limit],
                read_aggregate,
            )
        }
        .map_err(|error| format!("聚合渠道状态窗口失败: {error}"))?;

    let timeline =
        channel_status_timeline_from_connection(connection, monitor_id, since_ms, timeline_limit)?;
    let latest = timeline.first();
    let latest_error_message =
        latest_channel_status_error(connection, monitor_id, since_ms, timeline_limit)
            .ok()
            .flatten();

    Ok(ChannelStatusWindowFacts {
        total_count,
        success_count,
        failure_count,
        warning_count,
        avg_latency_ms,
        avg_endpoint_ping_ms: None,
        last_checked_at: latest.map(|point| point.checked_at.clone()),
        latest_status: latest.map(|point| point.status.clone()),
        latest_error_message,
        timeline,
    })
}

fn channel_status_timeline_from_connection(
    connection: &Connection,
    monitor_id: &str,
    since_ms: Option<i64>,
    limit: i64,
) -> Result<Vec<ChannelStatusTimelinePoint>, String> {
    let timeline_sql = if since_ms.is_some() {
        "SELECT status, COALESCE(latency_ms, duration_ms), COALESCE(finished_at, started_at)
           FROM channel_monitor_runs
          WHERE monitor_id = ?1 AND CAST(started_at AS INTEGER) >= ?2
          ORDER BY CAST(started_at AS INTEGER) DESC
          LIMIT ?3"
    } else {
        "SELECT status, COALESCE(latency_ms, duration_ms), COALESCE(finished_at, started_at)
           FROM channel_monitor_runs
          WHERE monitor_id = ?1
          ORDER BY CAST(started_at AS INTEGER) DESC
          LIMIT ?2"
    };

    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(ChannelStatusTimelinePoint {
            status: row.get(0)?,
            latency_ms: row.get(1)?,
            endpoint_ping_ms: None,
            checked_at: row.get(2)?,
        })
    };

    let mut statement = connection
        .prepare(timeline_sql)
        .map_err(|error| format!("读取渠道状态时间线失败: {error}"))?;
    let rows = if let Some(since_ms) = since_ms {
        statement.query_map(params![monitor_id, since_ms, limit], map_row)
    } else {
        statement.query_map(params![monitor_id, limit], map_row)
    }
    .map_err(|error| format!("查询渠道状态时间线失败: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析渠道状态时间线失败: {error}"))
}

fn latest_channel_status_error(
    connection: &Connection,
    monitor_id: &str,
    since_ms: Option<i64>,
    limit: i64,
) -> Result<Option<String>, String> {
    let error_sql = if since_ms.is_some() {
        "SELECT error_message
           FROM channel_monitor_runs
          WHERE monitor_id = ?1
            AND CAST(started_at AS INTEGER) >= ?2
            AND error_message IS NOT NULL
            AND TRIM(error_message) != ''
          ORDER BY CAST(started_at AS INTEGER) DESC
          LIMIT 1"
    } else {
        "WITH latest_runs AS (
            SELECT error_message, started_at
              FROM channel_monitor_runs
             WHERE monitor_id = ?1
             ORDER BY CAST(started_at AS INTEGER) DESC
             LIMIT ?2
         )
         SELECT error_message
           FROM latest_runs
          WHERE error_message IS NOT NULL
            AND TRIM(error_message) != ''
          ORDER BY CAST(started_at AS INTEGER) DESC
          LIMIT 1"
    };

    if let Some(since_ms) = since_ms {
        connection.query_row(error_sql, params![monitor_id, since_ms], |row| row.get(0))
    } else {
        connection.query_row(error_sql, params![monitor_id, limit], |row| row.get(0))
    }
    .optional()
    .map_err(|error| format!("读取渠道状态最近错误失败: {error}"))
}

fn insert_channel_monitor_run_in_connection(
    connection: &Connection,
    input: CreateChannelMonitorRunInput,
) -> Result<ChannelMonitorRun, String> {
    let monitor = channel_monitor_by_id(connection, &input.monitor_id)?;
    if input.template_id != monitor.template_id || input.station_id != monitor.station_id {
        return Err("Channel monitor run target does not match monitor".to_string());
    }
    match monitor.target_type.as_str() {
        "station_key" => {
            if input.station_key_id != monitor.station_key_id {
                return Err("Channel monitor run target does not match monitor".to_string());
            }
        }
        "station" => {
            if let Some(station_key_id) = input.station_key_id.as_deref() {
                validate_station_key_belongs_to_station(
                    connection,
                    &monitor.station_id,
                    station_key_id,
                )?;
            }
        }
        _ => return Err("Channel monitor target_type must be station_key or station".to_string()),
    }
    validate_channel_monitor_run_input(&input)?;
    let id = generate_id("channel_monitor_run");

    connection
        .execute(
            "INSERT INTO channel_monitor_runs (
                id, monitor_id, template_id, station_id, station_key_id, status,
                started_at, finished_at, duration_ms, http_status, latency_ms,
                response_model, fallback_model, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id,
                input.monitor_id,
                input.template_id,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                input.status.trim(),
                input.started_at,
                normalize_optional_string(input.finished_at),
                input.duration_ms,
                input.http_status,
                input.latency_ms,
                normalize_optional_string(input.response_model),
                normalize_optional_string(input.fallback_model),
                redact_optional_text(input.error_message),
                now_string(),
            ],
        )
        .map_err(|error| format!("保存通道监控运行记录失败: {error}"))?;

    connection
        .query_row(
            "SELECT id, monitor_id, template_id, station_id, station_key_id, status,
                    started_at, finished_at, duration_ms, http_status, latency_ms,
                    response_model, fallback_model, error_message, created_at
               FROM channel_monitor_runs
              WHERE id = ?1",
            params![id],
            row_to_channel_monitor_run,
        )
        .optional()
        .map_err(|error| format!("读取通道监控运行记录失败: {error}"))?
        .ok_or_else(|| "Channel monitor run does not exist".to_string())
}

fn update_channel_monitor_after_run_in_connection(
    connection: &Connection,
    id: &str,
    status: &str,
    finished_at: &str,
    error_message: Option<&str>,
) -> Result<(), String> {
    channel_monitor_by_id(connection, id)?;
    match status.trim() {
        "success" | "warning" | "failed" | "skipped" => {}
        _ => {
            return Err(
                "Channel monitor last_status must be success, warning, failed, or skipped"
                    .to_string(),
            )
        }
    }
    parse_channel_monitor_run_time(finished_at, "finished_at")?;
    let updated = connection
        .execute(
            "UPDATE channel_monitors
                SET last_run_at = ?1,
                    last_status = ?2,
                    last_error_message = ?3,
                    updated_at = ?4
              WHERE id = ?5",
            params![
                finished_at,
                status.trim(),
                redact_optional_text(error_message.map(ToString::to_string)),
                now_string(),
                id,
            ],
        )
        .map_err(|error| format!("Update channel monitor run summary failed: {error}"))?;
    if updated == 0 {
        return Err("Channel monitor does not exist".to_string());
    }
    Ok(())
}

fn schedule_next_channel_monitor_run_in_connection(
    connection: &Connection,
    id: &str,
) -> Result<String, String> {
    let monitor = channel_monitor_by_id(connection, id)?;
    let now = now_millis_for_services() as i64;
    let next_run_at = channel_monitor_next_run_at(
        &monitor.id,
        now,
        monitor.interval_seconds,
        monitor.jitter_seconds,
    )
    .to_string();
    connection
        .execute(
            "UPDATE channel_monitors
                SET next_run_at = ?1,
                    updated_at = ?2
              WHERE id = ?3",
            params![next_run_at, now_string(), id],
        )
        .map_err(|error| format!("Schedule next channel monitor run failed: {error}"))?;
    Ok(next_run_at)
}

fn due_channel_monitors_from_connection(
    connection: &Connection,
    now: &str,
) -> Result<Vec<ChannelMonitor>, String> {
    let now_ms = parse_channel_monitor_run_time(now, "now")?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, target_type, station_id, station_key_id, template_id,
                    enabled, interval_seconds, jitter_seconds, timeout_seconds,
                    max_concurrency, consecutive_failure_threshold, fallback_models_json,
                    note, created_at, updated_at
               FROM channel_monitors
              WHERE enabled = 1
                AND (next_run_at IS NULL OR CAST(next_run_at AS INTEGER) <= ?1)
              ORDER BY COALESCE(CAST(next_run_at AS INTEGER), 0) ASC, created_at ASC",
        )
        .map_err(|error| format!("Read due channel monitors failed: {error}"))?;
    let monitors = statement
        .query_map(params![now_ms], row_to_channel_monitor)
        .map_err(|error| format!("Query due channel monitors failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Parse due channel monitors failed: {error}"))?;
    Ok(monitors)
}

fn channel_monitor_next_run_at(
    monitor_id: &str,
    now_ms: i64,
    interval_seconds: i64,
    jitter_seconds: i64,
) -> i64 {
    let jitter_ms = if jitter_seconds <= 0 {
        0
    } else {
        let mut hasher = DefaultHasher::new();
        monitor_id.hash(&mut hasher);
        now_ms.hash(&mut hasher);
        (hasher.finish() % ((jitter_seconds as u64 * 1000) + 1)) as i64
    };
    now_ms + interval_seconds.max(1) * 1000 + jitter_ms
}

fn row_to_station(row: &rusqlite::Row<'_>) -> rusqlite::Result<Station> {
    let api_key: String = row.get("api_key")?;
    let secret_masked: Option<String> = row.get("api_key_masked")?;
    let api_key_secret_id: Option<String> = row.get("api_key_secret_id")?;
    let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
    let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();

    Ok(Station {
        id: row.get("id")?,
        name: row.get("name")?,
        station_type: row.get("station_type")?,
        website_url: row.get("website_url")?,
        api_base_url: row.get("api_base_url")?,
        endpoint_revision: row.get("endpoint_revision")?,
        collector_proxy_mode: row.get("collector_proxy_mode")?,
        collector_proxy_url: row.get("collector_proxy_url")?,
        api_key_masked,
        api_key_present,
        key_count: row.get("key_count")?,
        enabled: i64_to_bool(row.get("enabled")?),
        priority: row.get("priority")?,
        credit_per_cny: row.get("credit_per_cny")?,
        balance_raw: row.get("balance_raw")?,
        balance_cny: row.get("balance_cny")?,
        low_balance_threshold_cny: row.get("low_balance_threshold_cny")?,
        collection_interval_minutes: row.get("collection_interval_minutes")?,
        status: row.get("status")?,
        latency_ms: row.get("latency_ms")?,
        last_checked_at: row.get("last_checked_at")?,
        last_pricing_fetched_at: row.get("last_pricing_fetched_at")?,
        note: row.get("note")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn list_stations_from_connection(connection: &Connection) -> Result<Vec<Station>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, station_type, website_url, api_base_url, endpoint_revision,
                    api_key, upstream_api_format,
                    (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
                    enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    collection_interval_minutes,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id) AS api_key_masked,
                    api_key_secret_id,
                    collector_proxy_mode, collector_proxy_url
               FROM stations
              ORDER BY priority ASC, created_at ASC",
        )
        .map_err(|error| format!("读取站点列表失败: {error}"))?;

    let stations = statement
        .query_map([], row_to_station)
        .map_err(|error| format!("查询站点列表失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析站点列表失败: {error}"))?;

    Ok(stations)
}

fn station_by_id(connection: &Connection, id: &str) -> Result<Station, String> {
    connection
        .query_row(
            "SELECT id, name, station_type, website_url, api_base_url, endpoint_revision,
                    api_key, upstream_api_format,
                    (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
                    enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    collection_interval_minutes,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id) AS api_key_masked,
                    api_key_secret_id,
                    collector_proxy_mode, collector_proxy_url
               FROM stations
              WHERE id = ?1",
            params![id],
            row_to_station,
        )
        .optional()
        .map_err(|error| format!("读取站点失败: {error}"))?
        .ok_or_else(|| "站点不存在".to_string())
}

fn due_station_collectors_from_connection(
    connection: &Connection,
    now: &str,
) -> Result<Vec<Station>, String> {
    let now_ms = parse_channel_monitor_run_time(now, "now")?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, station_type, website_url, api_base_url, endpoint_revision,
                    api_key, upstream_api_format,
                    (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
                    enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    collection_interval_minutes,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id) AS api_key_masked,
                    api_key_secret_id,
                    collector_proxy_mode, collector_proxy_url
               FROM stations
              WHERE enabled = 1
                AND (
                  COALESCE(last_pricing_fetched_at, last_checked_at) IS NULL
                  OR CAST(COALESCE(last_pricing_fetched_at, last_checked_at) AS INTEGER)
                    + CAST(collection_interval_minutes AS INTEGER) * 60000 <= ?1
                )
              ORDER BY
                COALESCE(CAST(last_pricing_fetched_at AS INTEGER), CAST(last_checked_at AS INTEGER), 0) ASC,
                priority ASC,
                created_at ASC",
        )
        .map_err(|error| format!("Read due station collectors failed: {error}"))?;
    let stations = statement
        .query_map(params![now_ms], row_to_station)
        .map_err(|error| format!("Query due station collectors failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Parse due station collectors failed: {error}"))?;
    Ok(stations)
}

fn create_station_in_connection(
    connection: &Connection,
    input: CreateStationInput,
    data_key: Option<&[u8; 32]>,
) -> Result<Station, String> {
    let id = generate_id("station");
    let now = now_string();
    let next_priority = next_station_priority(connection)?;
    let plaintext_api_key = input.api_key.trim().to_string();
    let endpoints = normalize_station_endpoints(&input.website_url, &input.api_base_url)?;
    let collector_proxy_mode = normalize_proxy_mode(&input.collector_proxy_mode, true);
    let collector_proxy_url = normalize_proxy_url(input.collector_proxy_url);
    let stored_api_key = if data_key.is_some() {
        "".to_string()
    } else {
        plaintext_api_key.clone()
    };

    connection
        .execute(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, endpoint_revision,
                api_key, api_key_secret_id,
                collector_proxy_mode, collector_proxy_url, enabled, priority,
                credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                collection_interval_minutes,
                status, latency_ms, last_checked_at, last_pricing_fetched_at,
                note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, NULL, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12,
                ?13, ?14, NULL, NULL, NULL, ?15, ?16, ?17)",
            params![
                id,
                input.name.trim(),
                input.station_type,
                endpoints.website_url,
                endpoints.api_base_url,
                stored_api_key,
                collector_proxy_mode,
                collector_proxy_url,
                bool_to_i64(input.enabled),
                next_priority,
                input.credit_per_cny,
                input.low_balance_threshold_cny,
                input.collection_interval_minutes,
                if input.enabled {
                    "unchecked"
                } else {
                    "disabled"
                },
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("创建站点失败: {error}"))?;

    if let Some(data_key) = data_key.filter(|_| !plaintext_api_key.is_empty()) {
        let secret_id = upsert_secret_in_connection(
            connection,
            data_key,
            "station",
            &id,
            "api_key",
            &plaintext_api_key,
        )?;
        connection
            .execute(
                "UPDATE stations SET api_key_secret_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![secret_id, now_string(), id],
            )
            .map_err(|error| format!("保存站点加密 API Key 失败: {error}"))?;
    }

    if !plaintext_api_key.is_empty() {
        create_station_key_in_connection_with_data_key(
            connection,
            CreateStationKeyInput {
                station_id: id.clone(),
                name: "Default Key".to_string(),
                api_key: input.api_key,
                enabled: input.enabled,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: Some("由站点默认 API Key 创建。".to_string()),
            },
            data_key,
        )?;
    }

    station_by_id(connection, &id)
}

fn update_station_in_connection(
    connection: &Connection,
    input: UpdateStationInput,
    data_key: Option<&[u8; 32]>,
) -> Result<Station, String> {
    let existing: Option<(String, Option<String>, String, String, i64)> = connection
        .query_row(
            "SELECT api_key, api_key_secret_id, website_url, api_base_url, endpoint_revision
               FROM stations WHERE id = ?1",
            params![input.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取站点 API Key 失败: {error}"))?;

    let Some((
        existing_api_key,
        existing_secret_id,
        existing_website_url,
        existing_api_base_url,
        existing_endpoint_revision,
    )) = existing
    else {
        return Err("站点不存在，无法更新".to_string());
    };

    let new_api_key = input
        .api_key
        .as_ref()
        .map(|api_key| api_key.trim())
        .filter(|api_key| !api_key.is_empty());
    let (next_api_key, next_secret_id) = if let Some(data_key) = data_key {
        let secret_id = match new_api_key {
            Some(api_key) => Some(upsert_secret_in_connection(
                connection, data_key, "station", &input.id, "api_key", api_key,
            )?),
            None => existing_secret_id,
        };
        ("".to_string(), secret_id)
    } else {
        (
            new_api_key
                .map(ToString::to_string)
                .unwrap_or(existing_api_key),
            existing_secret_id,
        )
    };
    let collector_proxy_mode = normalize_proxy_mode(&input.collector_proxy_mode, true);
    let collector_proxy_url = normalize_proxy_url(input.collector_proxy_url);
    let endpoints = normalize_station_endpoints(&input.website_url, &input.api_base_url)?;
    let website_url_changed = endpoints.website_url != existing_website_url;
    let api_base_url_changed = endpoints.api_base_url != existing_api_base_url;
    let endpoints_changed = website_url_changed || api_base_url_changed;
    let website_origin_changed =
        endpoints_changed && !same_origin(&existing_website_url, &endpoints.website_url)?;
    let api_origin_changed =
        endpoints_changed && !same_origin(&existing_api_base_url, &endpoints.api_base_url)?;
    let endpoint_revision = if endpoints_changed {
        existing_endpoint_revision.max(1) + 1
    } else {
        existing_endpoint_revision.max(1)
    };
    let next_enabled = input.enabled && !api_origin_changed;
    let now = now_string();

    connection
        .execute(
            "UPDATE stations
                SET name = ?1,
                    station_type = ?2,
                    website_url = ?3,
                    api_base_url = ?4,
                    endpoint_revision = ?5,
                    api_key = ?6,
                    api_key_secret_id = ?7,
                    collector_proxy_mode = ?8,
                    collector_proxy_url = ?9,
                    enabled = ?10,
                    credit_per_cny = ?11,
                    low_balance_threshold_cny = ?12,
                    collection_interval_minutes = ?13,
                    status = CASE WHEN ?10 = 0 THEN 'disabled'
                                  WHEN ?17 = 1 THEN 'unchecked'
                                  WHEN status = 'disabled' THEN 'unchecked'
                                  ELSE status END,
                    note = ?14,
                    last_checked_at = CASE WHEN ?17 = 1 THEN NULL ELSE last_checked_at END,
                    last_pricing_fetched_at = CASE WHEN ?17 = 1 THEN NULL ELSE last_pricing_fetched_at END,
                    updated_at = ?15
              WHERE id = ?16",
            params![
                input.name.trim(),
                input.station_type,
                endpoints.website_url,
                endpoints.api_base_url,
                endpoint_revision,
                next_api_key,
                next_secret_id,
                collector_proxy_mode,
                collector_proxy_url,
                bool_to_i64(next_enabled),
                input.credit_per_cny,
                input.low_balance_threshold_cny,
                input.collection_interval_minutes,
                normalize_optional_string(input.note),
                now,
                input.id,
                bool_to_i64(endpoints_changed),
            ],
        )
        .map_err(|error| format!("更新站点失败: {error}"))?;

    if website_origin_changed {
        clear_station_origin_bound_login_material(connection, &input.id)?;
    }
    if api_base_url_changed {
        clear_station_endpoint_health_state(connection, &input.id)?;
    }

    station_by_id(connection, &input.id)
}

fn clear_station_endpoint_health_state(
    connection: &Connection,
    station_id: &str,
) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM station_endpoint_health WHERE station_id = ?1",
            params![station_id],
        )
        .map_err(|error| format!("清理站点端点健康状态失败: {error}"))?;
    connection
        .execute(
            "DELETE FROM station_key_health
              WHERE station_key_id IN (
                    SELECT id FROM station_keys WHERE station_id = ?1
              )",
            params![station_id],
        )
        .map_err(|error| format!("清理 Station Key 健康状态失败: {error}"))?;
    Ok(())
}

fn clear_station_origin_bound_login_material(
    connection: &Connection,
    station_id: &str,
) -> Result<(), String> {
    let secret_ids = connection
        .query_row(
            "SELECT login_password_secret_id, access_token_secret_id,
                    refresh_token_secret_id, cookie_secret_id
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                Ok([
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ])
            },
        )
        .optional()
        .map_err(|error| format!("读取站点登录凭据失败: {error}"))?
        .unwrap_or([None, None, None, None]);

    connection
        .execute(
            "UPDATE station_credentials
                SET login_password = NULL,
                    login_password_secret_id = NULL,
                    remember_password = 0,
                    login_status = 'unknown',
                    login_error = NULL,
                    last_login_at = NULL,
                    session_status = 'none',
                    session_expires_at = NULL,
                    access_token_secret_id = NULL,
                    refresh_token_secret_id = NULL,
                    cookie_secret_id = NULL,
                    newapi_user_id = NULL,
                    token_expires_at = NULL,
                    token_refreshed_at = NULL,
                    session_source = 'none',
                    updated_at = ?1
              WHERE station_id = ?2",
            params![now_string(), station_id],
        )
        .map_err(|error| format!("清理站点登录凭据失败: {error}"))?;

    for secret_id in secret_ids.into_iter().flatten() {
        delete_unreferenced_secret_by_id(connection, &secret_id)?;
    }
    Ok(())
}

fn validate_station_exists(connection: &Connection, station_id: &str) -> Result<(), String> {
    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM stations WHERE id = ?1",
            params![station_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取站点失败: {error}"))?;

    if exists.is_none() {
        return Err("站点不存在".to_string());
    }

    Ok(())
}

fn station_endpoint_revision(connection: &Connection, station_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT endpoint_revision FROM stations WHERE id = ?1",
            params![station_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("read station endpoint revision failed: {error}"))?
        .ok_or_else(|| "station does not exist".to_string())
}

fn ensure_station_endpoint_revision(
    connection: &Connection,
    station_id: &str,
    expected_revision: i64,
) -> Result<(), String> {
    let current = station_endpoint_revision(connection, station_id)?;
    if current != expected_revision {
        return Err("station_endpoint_revision_changed".to_string());
    }
    Ok(())
}

fn station_key_endpoint_revision(
    connection: &Connection,
    station_key_id: &str,
) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT s.endpoint_revision
               FROM station_keys k
               JOIN stations s ON s.id = k.station_id
              WHERE k.id = ?1",
            params![station_key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("read station key endpoint revision failed: {error}"))?
        .ok_or_else(|| "station key does not exist".to_string())
}

fn schema_columns(connection: &Connection, table: &str) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("read {table} schema failed: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get(1))
        .map_err(|error| format!("query {table} schema failed: {error}"))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| format!("parse {table} schema failed: {error}"))?;
    Ok(columns)
}

fn schema_table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists != 0)
        .map_err(|error| format!("check {table} schema failed: {error}"))
}

fn migrated_legacy_station_endpoints(legacy_url: &str) -> Result<(String, String), String> {
    let api_base_url = legacy_api_base_url(legacy_url)?;
    let legacy = normalize_station_endpoints(legacy_url, &api_base_url)?;
    let website_url = if legacy.website_url.ends_with("/v1") {
        legacy_website_url(&legacy.website_url)?
    } else {
        legacy.website_url
    };
    let endpoints = normalize_station_endpoints(&website_url, &legacy.api_base_url)?;
    Ok((endpoints.website_url, endpoints.api_base_url))
}

fn migrate_station_endpoint_urls(connection: &Connection) -> Result<(), String> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("start station endpoint migration failed: {error}"))?;
    let columns = schema_columns(&transaction, "stations")?;
    if columns.is_empty() {
        return Err("station endpoint schema conflict: stations table is missing".to_string());
    }

    let has_base_url = columns.contains("base_url");
    let has_website_url = columns.contains("website_url");
    let has_api_base_url = columns.contains("api_base_url");
    let endpoint_rows = match (has_base_url, has_website_url, has_api_base_url) {
        (true, false, false) => {
            let mut statement = transaction
                .prepare("SELECT id, base_url FROM stations")
                .map_err(|error| format!("read legacy station endpoints failed: {error}"))?;
            let rows = statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|error| format!("query legacy station endpoints failed: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("parse legacy station endpoints failed: {error}"))?;
            rows.into_iter()
                .map(|(id, legacy_url)| {
                    let (website_url, api_base_url) =
                        migrated_legacy_station_endpoints(&legacy_url)?;
                    Ok((id, website_url, api_base_url))
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        (true, false, true) => {
            let mut statement = transaction
                .prepare("SELECT id, base_url, api_base_url FROM stations")
                .map_err(|error| format!("read transitional station endpoints failed: {error}"))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| format!("query transitional station endpoints failed: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("parse transitional station endpoints failed: {error}"))?;
            rows.into_iter()
                .map(|(id, legacy_url, stored_api_base_url)| {
                    let (website_url, expected_api_base_url) =
                        migrated_legacy_station_endpoints(&legacy_url)?;
                    let expected = normalize_station_endpoints(&website_url, &expected_api_base_url)?;
                    let stored =
                        normalize_station_endpoints(&expected.website_url, &stored_api_base_url)?;
                    if stored.api_base_url != expected.api_base_url {
                        return Err(format!(
                            "station endpoint conflict for {id}: legacy URL derives {}, stored API URL is {}",
                            expected.api_base_url, stored.api_base_url
                        ));
                    }
                    Ok((id, expected.website_url, expected.api_base_url))
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        (false, true, true) => {
            let mut statement = transaction
                .prepare("SELECT id, website_url, api_base_url FROM stations")
                .map_err(|error| format!("read station endpoints failed: {error}"))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| format!("query station endpoints failed: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("parse station endpoints failed: {error}"))?;
            rows.into_iter()
                .map(|(id, website_url, api_base_url)| {
                    let endpoints = normalize_station_endpoints(&website_url, &api_base_url)?;
                    Ok((id, endpoints.website_url, endpoints.api_base_url))
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        (true, true, _) => {
            return Err(
                "station endpoint schema conflict: base_url and website_url coexist".to_string(),
            )
        }
        _ => {
            return Err(format!(
                "station endpoint schema conflict: unsupported columns base_url={has_base_url}, website_url={has_website_url}, api_base_url={has_api_base_url}"
            ))
        }
    };

    if has_base_url {
        transaction
            .execute_batch("ALTER TABLE stations RENAME COLUMN base_url TO website_url;")
            .map_err(|error| format!("rename legacy station URL failed: {error}"))?;
    }
    if !has_api_base_url {
        transaction
            .execute_batch("ALTER TABLE stations ADD COLUMN api_base_url TEXT NOT NULL DEFAULT '';")
            .map_err(|error| format!("add station API URL failed: {error}"))?;
    }
    if !columns.contains("endpoint_revision") {
        transaction
            .execute_batch(
                "ALTER TABLE stations ADD COLUMN endpoint_revision INTEGER NOT NULL DEFAULT 1;",
            )
            .map_err(|error| format!("add station endpoint revision failed: {error}"))?;
    }
    for (id, website_url, api_base_url) in endpoint_rows {
        transaction
            .execute(
                "UPDATE stations
                    SET website_url = ?2,
                        api_base_url = ?3,
                        endpoint_revision = CASE
                            WHEN endpoint_revision IS NULL OR endpoint_revision < 1 THEN 1
                            ELSE endpoint_revision
                        END
                  WHERE id = ?1",
                params![id, website_url, api_base_url],
            )
            .map_err(|error| format!("backfill station endpoints failed: {error}"))?;
    }

    if columns.contains("upstream_api_base_path") {
        transaction
            .execute_batch("ALTER TABLE stations DROP COLUMN upstream_api_base_path;")
            .map_err(|error| format!("remove legacy API base path failed: {error}"))?;
    }
    for table in [
        "collector_snapshots",
        "collector_runs",
        "station_endpoint_health",
        "station_key_health",
    ] {
        if !schema_table_exists(&transaction, table)? {
            continue;
        }
        let table_columns = schema_columns(&transaction, table)?;
        if !table_columns.contains("endpoint_revision") {
            transaction
                .execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN endpoint_revision INTEGER NOT NULL DEFAULT 1;"
                ))
                .map_err(|error| format!("add {table} endpoint revision failed: {error}"))?;
        }
        transaction
            .execute(
                &format!(
                    "UPDATE {table} SET endpoint_revision = 1 WHERE endpoint_revision IS NULL OR endpoint_revision < 1"
                ),
                [],
            )
            .map_err(|error| format!("backfill {table} endpoint revision failed: {error}"))?;
    }

    transaction
        .commit()
        .map_err(|error| format!("commit station endpoint migration failed: {error}"))
}

fn migrate_station_proxy_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(stations)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|column| column == "upstream_api_format") {
        connection.execute(
            "ALTER TABLE stations ADD COLUMN upstream_api_format TEXT NOT NULL DEFAULT 'auto'",
            [],
        )?;
    }
    if !rows
        .iter()
        .any(|column| column == "collection_interval_minutes")
    {
        connection.execute(
            "ALTER TABLE stations ADD COLUMN collection_interval_minutes INTEGER NOT NULL DEFAULT 5",
            [],
        )?;
    }
    if !rows.iter().any(|column| column == "collector_proxy_mode") {
        connection.execute(
            "ALTER TABLE stations ADD COLUMN collector_proxy_mode TEXT NOT NULL DEFAULT 'inherit'",
            [],
        )?;
    }
    if !rows.iter().any(|column| column == "collector_proxy_url") {
        connection.execute(
            "ALTER TABLE stations ADD COLUMN collector_proxy_url TEXT",
            [],
        )?;
    }

    Ok(())
}

fn validate_station_key_exists(
    connection: &Connection,
    station_key_id: &str,
) -> Result<(), String> {
    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?;

    if exists.is_none() {
        return Err("Station Key 不存在".to_string());
    }

    Ok(())
}

fn migrate_request_log_route_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|column| column == "route_policy") {
        connection.execute("ALTER TABLE request_logs ADD COLUMN route_policy TEXT", [])?;
    }
    if !rows.iter().any(|column| column == "route_reason") {
        connection.execute("ALTER TABLE request_logs ADD COLUMN route_reason TEXT", [])?;
    }
    if !rows
        .iter()
        .any(|column| column == "rejected_candidates_json")
    {
        connection.execute(
            "ALTER TABLE request_logs ADD COLUMN rejected_candidates_json TEXT",
            [],
        )?;
    }

    Ok(())
}

fn migrate_request_log_cost_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    for column in [
        "prompt_tokens",
        "completion_tokens",
        "total_tokens",
        "estimated_input_cost",
        "estimated_output_cost",
        "estimated_total_cost",
        "base_input_cost",
        "base_output_cost",
        "base_fixed_cost",
        "base_total_cost",
        "cost_currency",
        "pricing_rule_id",
        "pricing_source",
        "cost_status",
    ] {
        if !rows.iter().any(|existing| existing == column) {
            let sql = format!(
                "ALTER TABLE request_logs ADD COLUMN {column} {}",
                if column.ends_with("_tokens") {
                    "INTEGER"
                } else if column.ends_with("_cost") {
                    "REAL"
                } else {
                    "TEXT"
                }
            );
            connection.execute(&sql, [])?;
        }
    }

    Ok(())
}

fn migrate_request_log_economic_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    for column in [
        "group_binding_id",
        "normalization_status",
        "balance_scope",
        "economic_context_json",
    ] {
        if !rows.iter().any(|existing| existing == column) {
            connection.execute(
                &format!("ALTER TABLE request_logs ADD COLUMN {column} TEXT"),
                [],
            )?;
        }
    }

    Ok(())
}

fn migrate_request_log_lifecycle_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|existing| existing == "lifecycle_status") {
        connection.execute(
            "ALTER TABLE request_logs ADD COLUMN lifecycle_status TEXT",
            [],
        )?;
    }

    Ok(())
}

fn migrate_request_log_observability_columns(connection: &Connection) -> rusqlite::Result<()> {
    for (column, column_type) in [
        ("cache_creation_tokens", "INTEGER"),
        ("cache_read_tokens", "INTEGER"),
        ("reasoning_effort", "TEXT"),
        ("first_token_ms", "INTEGER"),
        ("billing_mode", "TEXT"),
    ] {
        add_column_if_missing(connection, "request_logs", column, column_type)?;
    }
    Ok(())
}

fn migrate_remote_key_tables(connection: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'remote_station_keys'
        )",
        [],
        |row| row.get(0),
    )?;

    if table_exists && !remote_station_keys_have_required_foreign_keys(connection)? {
        rebuild_remote_station_keys_table_with_foreign_keys(connection)?;
        return Ok(());
    }

    create_remote_station_keys_table(connection)?;
    create_remote_station_keys_indexes(connection)
}

fn remote_station_keys_have_required_foreign_keys(
    connection: &Connection,
) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare("PRAGMA foreign_key_list(remote_station_keys)")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let has_station_fk = rows.iter().any(|(table, from, on_delete)| {
        table == "stations" && from == "station_id" && on_delete == "CASCADE"
    });
    let has_station_key_fk = rows.iter().any(|(table, from, on_delete)| {
        table == "station_keys" && from == "matched_station_key_id" && on_delete == "SET NULL"
    });

    Ok(has_station_fk && has_station_key_fk)
}

fn rebuild_remote_station_keys_table_with_foreign_keys(
    connection: &Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch("SAVEPOINT remote_station_keys_migration;")?;
    let result = rebuild_remote_station_keys_table_with_foreign_keys_inner(connection);

    match result {
        Ok(()) => connection.execute_batch("RELEASE SAVEPOINT remote_station_keys_migration;"),
        Err(error) => {
            let _ = connection.execute_batch(
                "ROLLBACK TO SAVEPOINT remote_station_keys_migration;
                 RELEASE SAVEPOINT remote_station_keys_migration;",
            );
            Err(error)
        }
    }
}

fn rebuild_remote_station_keys_table_with_foreign_keys_inner(
    connection: &Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        ALTER TABLE remote_station_keys
            RENAME TO remote_station_keys_no_fk_migration;
        "#,
    )?;
    create_remote_station_keys_table(connection)?;
    connection.execute_batch(
        r#"
        INSERT INTO remote_station_keys (
            id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
            api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
            rate_source, created_at_remote, last_used_at, raw_source, match_status,
            matched_station_key_id, match_confidence, collected_at, updated_at
        )
        SELECT
            remote_station_keys_no_fk_migration.id,
            remote_station_keys_no_fk_migration.station_id,
            remote_station_keys_no_fk_migration.remote_key_id_hash,
            remote_station_keys_no_fk_migration.remote_key_name,
            remote_station_keys_no_fk_migration.api_key_masked,
            remote_station_keys_no_fk_migration.api_key_fingerprint,
            remote_station_keys_no_fk_migration.group_id_hash,
            remote_station_keys_no_fk_migration.group_name,
            remote_station_keys_no_fk_migration.tier_label,
            remote_station_keys_no_fk_migration.rate_multiplier,
            remote_station_keys_no_fk_migration.rate_source,
            remote_station_keys_no_fk_migration.created_at_remote,
            remote_station_keys_no_fk_migration.last_used_at,
            remote_station_keys_no_fk_migration.raw_source,
            CASE
                WHEN station_keys.id IS NULL
                     AND remote_station_keys_no_fk_migration.match_status = 'matched'
                THEN 'unbound'
                ELSE remote_station_keys_no_fk_migration.match_status
            END,
            station_keys.id,
            CASE
                WHEN station_keys.id IS NULL
                     AND remote_station_keys_no_fk_migration.match_status = 'matched'
                THEN 0.0
                ELSE remote_station_keys_no_fk_migration.match_confidence
            END,
            remote_station_keys_no_fk_migration.collected_at,
            remote_station_keys_no_fk_migration.updated_at
          FROM remote_station_keys_no_fk_migration
          INNER JOIN stations
                  ON stations.id = remote_station_keys_no_fk_migration.station_id
          LEFT JOIN station_keys
                 ON station_keys.id = remote_station_keys_no_fk_migration.matched_station_key_id
                AND station_keys.station_id = remote_station_keys_no_fk_migration.station_id;

        DROP TABLE remote_station_keys_no_fk_migration;
        "#,
    )?;
    create_remote_station_keys_indexes(connection)
}

fn create_remote_station_keys_table(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS remote_station_keys (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            remote_key_id_hash TEXT,
            remote_key_name TEXT,
            api_key_masked TEXT,
            api_key_fingerprint TEXT,
            group_id_hash TEXT,
            group_name TEXT,
            tier_label TEXT,
            rate_multiplier REAL,
            rate_source TEXT,
            created_at_remote TEXT,
            last_used_at TEXT,
            raw_source TEXT NOT NULL,
            match_status TEXT NOT NULL,
            matched_station_key_id TEXT,
            match_confidence REAL NOT NULL,
            collected_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(matched_station_key_id) REFERENCES station_keys(id) ON DELETE SET NULL
        );
        "#,
    )
}

fn create_remote_station_keys_indexes(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_remote_station_keys_station
            ON remote_station_keys(station_id, collected_at DESC);

        CREATE INDEX IF NOT EXISTS idx_remote_station_keys_matched_key
            ON remote_station_keys(matched_station_key_id);
        "#,
    )
}

fn migrate_pricing_tables(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS pricing_rules (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            group_name TEXT,
            tier_label TEXT,
            model TEXT NOT NULL,
            input_price REAL,
            output_price REAL,
            fixed_price REAL,
            currency TEXT NOT NULL,
            unit TEXT NOT NULL,
            price_type TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.5,
            enabled INTEGER NOT NULL DEFAULT 1,
            note TEXT,
            collected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_pricing_rules_station_model
            ON pricing_rules(station_id, model, enabled, updated_at DESC);

        CREATE TABLE IF NOT EXISTS model_base_prices (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            input_price REAL,
            output_price REAL,
            currency TEXT NOT NULL,
            unit TEXT NOT NULL,
            source_url TEXT NOT NULL,
            source_label TEXT NOT NULL,
            source_checked_at TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            built_in INTEGER NOT NULL DEFAULT 0,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_model_base_prices_model
            ON model_base_prices(model, enabled, updated_at DESC);

        CREATE TABLE IF NOT EXISTS balance_snapshots (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            scope TEXT NOT NULL,
            value REAL,
            currency TEXT NOT NULL,
            credit_unit TEXT,
            used_value REAL,
            total_value REAL,
            today_request_count INTEGER,
            total_request_count INTEGER,
            today_consumption REAL,
            total_consumption REAL,
            today_base_consumption REAL,
            total_base_consumption REAL,
            today_token_count INTEGER,
            total_token_count INTEGER,
            today_input_token_count INTEGER,
            today_output_token_count INTEGER,
            total_input_token_count INTEGER,
            total_output_token_count INTEGER,
            account_concurrency_limit INTEGER,
            low_balance_threshold REAL,
            status TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0.5,
            collected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_balance_snapshots_station_scope_updated
            ON balance_snapshots(station_id, scope, updated_at DESC);
        ",
    )
}

fn migrate_default_station_keys(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(
        "SELECT id, api_key, enabled, created_at
           FROM stations
          WHERE api_key IS NOT NULL
            AND TRIM(api_key) != ''
            AND NOT EXISTS (
                SELECT 1 FROM station_keys WHERE station_keys.station_id = stations.id
            )",
    )?;

    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    for row in rows {
        let (station_id, api_key, enabled, created_at) = row?;
        connection.execute(
            "INSERT INTO station_keys (
                id, station_id, name, api_key, enabled, priority, group_name, tier_label,
                status, last_checked_at, last_used_at, note, created_at, updated_at
             ) VALUES (?1, ?2, 'Default Key', ?3, ?4, 0, NULL, NULL, 'unchecked',
                NULL, NULL, '由 Phase 2 站点 API Key 自动迁移。', ?5, ?6)",
            params![
                generate_id("key"),
                station_id,
                api_key,
                enabled,
                created_at,
                now_string(),
            ],
        )?;
    }

    Ok(())
}

fn row_to_station_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<StationKey> {
    let api_key: String = row.get(3)?;
    let secret_masked: Option<String> = row.get(25)?;
    let api_key_secret_id: Option<String> = row.get(26)?;
    let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
    let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();

    Ok(StationKey {
        id: row.get(0)?,
        station_id: row.get(1)?,
        name: row.get(2)?,
        api_key_masked,
        api_key_present,
        enabled: i64_to_bool(row.get(4)?),
        priority: row.get(5)?,
        max_concurrency: row.get(6)?,
        load_factor: row.get(7)?,
        schedulable: i64_to_bool(row.get(8)?),
        group_name: row.get(9)?,
        tier_label: row.get(10)?,
        group_binding_id: row.get(11)?,
        group_id_hash: row.get(12)?,
        rate_multiplier: row.get(13)?,
        manual_rate_multiplier: row.get(14)?,
        manual_rate_updated_at: row.get(15)?,
        rate_source: row.get(16)?,
        rate_collected_at: row.get(17)?,
        balance_scope: row.get(18)?,
        status: row.get(19)?,
        last_checked_at: row.get(20)?,
        last_used_at: row.get(21)?,
        note: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
    })
}

fn list_station_keys_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<StationKey>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, name, api_key, enabled, priority,
                    max_concurrency, load_factor, schedulable,
                    group_name, tier_label,
                    group_binding_id, group_id_hash, rate_multiplier,
                    manual_rate_multiplier, manual_rate_updated_at, rate_source,
                    rate_collected_at, balance_scope,
                    status, last_checked_at, last_used_at, note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = station_keys.api_key_secret_id),
                    api_key_secret_id
               FROM station_keys
              WHERE station_id = ?1
              ORDER BY priority ASC, created_at ASC",
        )
        .map_err(|error| format!("读取 Station Key 列表失败: {error}"))?;

    let rows = statement
        .query_map(params![station_id], row_to_station_key)
        .map_err(|error| format!("查询 Station Key 失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Station Key 失败: {error}"))?;
    Ok(rows)
}

fn load_multiplier_source_facts_from_connection(
    connection: &Connection,
    station_key_id: &str,
) -> Result<MultiplierSourceFacts, String> {
    connection
        .query_row(
            "SELECT k.id, k.manual_rate_multiplier, k.manual_rate_updated_at,
                    COALESCE(cb.id, k.group_binding_id),
                    COALESCE(cb.group_id_hash, b.group_id_hash, k.group_id_hash),
                    COALESCE(cb.group_name, b.group_name, k.group_name),
                    COALESCE(cb.inferred_group_category, b.inferred_group_category),
                    COALESCE(cb.group_category_override, b.group_category_override),
                    k.rate_multiplier, k.rate_source, k.rate_collected_at,
                    COALESCE(cb.confidence, b.confidence, 0.8)
               FROM station_keys k
               LEFT JOIN station_group_bindings b ON b.id = k.group_binding_id
               LEFT JOIN station_group_bindings cb ON cb.id = (
                    SELECT canonical.id
                      FROM station_group_bindings canonical
                     WHERE canonical.station_id = k.station_id
                       AND canonical.binding_kind = 'station_group'
                       AND canonical.binding_status = 'available'
                       AND COALESCE(canonical.rate_source, '') != 'legacy_key_group'
                       AND lower(trim(canonical.group_name)) = lower(trim(COALESCE(b.group_name, k.group_name)))
                       AND b.id IS NOT NULL
                       AND (
                            b.binding_kind != 'station_group'
                            OR b.binding_status != 'available'
                            OR COALESCE(b.rate_source, '') = 'legacy_key_group'
                       )
                     ORDER BY canonical.updated_at DESC, canonical.created_at DESC, canonical.id ASC
                     LIMIT 1
               )
              WHERE k.id = ?1",
            params![station_key_id],
            |row| {
                let manual_rate_updated_at: Option<String> = row.get(2)?;
                let rate_collected_at: Option<String> = row.get(10)?;
                Ok(MultiplierSourceFacts {
                    station_key_id: row.get(0)?,
                    manual_rate_multiplier: row.get(1)?,
                    manual_rate_updated_at,
                    group_binding_id: row.get(3)?,
                    group_id_hash: row.get(4)?,
                    group_name: row.get(5)?,
                    inferred_group_category: row.get(6)?,
                    group_category_override: row.get(7)?,
                    collected_rate_multiplier: row.get(8)?,
                    collected_rate_source: row.get(9)?,
                    collected_rate_confidence: row.get(11)?,
                    collected_rate_collected_at_ms: parse_optional_millisecond_timestamp(
                        rate_collected_at.as_deref(),
                    ),
                    collected_rate_valid_until_ms: None,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取 Key 倍率事实失败: {error}"))?
        .ok_or_else(|| "Station Key 不存在，无法读取倍率事实".to_string())
}

struct SchedulerCandidateBaseRow {
    station_key_id: String,
    station_id: String,
    priority: i64,
    max_concurrency: i64,
    load_factor: Option<i64>,
    group_binding_id: Option<String>,
    group_id_hash: Option<String>,
    group_name: Option<String>,
    inferred_group_category: Option<String>,
    group_category_override: Option<String>,
    station_enabled: bool,
    key_enabled: bool,
    schedulable: bool,
    supports_chat_completions: bool,
    supports_responses: bool,
    supports_embeddings: bool,
    supports_stream: bool,
    supports_tools: bool,
    supports_vision: bool,
    supports_reasoning: bool,
    model_allowlist: Vec<String>,
    model_blocklist: Vec<String>,
    health_blocked: bool,
    balance_depleted: bool,
}

fn scheduler_row_group_type(row: &SchedulerCandidateBaseRow) -> Option<PricingGroupType> {
    pricing_group_type_from_binding_metadata(
        row.group_category_override.as_deref(),
        row.inferred_group_category.as_deref(),
        row.group_name.as_deref(),
    )
}

fn load_scheduler_candidates_from_connection(
    connection: &Connection,
    filter: &RoutingGroupFilter,
    now_ms: i64,
) -> Result<Vec<SchedulerCandidate>, String> {
    let advanced_settings = parse_scheduler_advanced_settings(&read_setting_or_default(
        connection,
        "scheduler_advanced_settings_json",
        "",
    )?)?;
    let group_rate_interval_minutes: u16 =
        parse_setting_or_default(connection, "group_rate_interval_minutes", "20")?;
    let group_rate_interval_ms = i64::from(group_rate_interval_minutes) * 60 * 1000;

    let mut sql = String::from(
        "SELECT
            k.id,
            k.station_id,
            COALESCE(k.routing_order, k.priority),
            COALESCE(cb.id, k.group_binding_id),
            COALESCE(cb.group_id_hash, b.group_id_hash, k.group_id_hash),
            COALESCE(cb.group_name, b.group_name, k.group_name),
            COALESCE(cb.inferred_group_category, b.inferred_group_category),
            COALESCE(cb.group_category_override, b.group_category_override),
            s.enabled,
            k.enabled,
            k.schedulable,
            COALESCE(c.supports_chat_completions, 1),
            COALESCE(c.supports_responses, 1),
            COALESCE(c.supports_embeddings, 0),
            COALESCE(c.supports_stream, 1),
            COALESCE(c.supports_tools, 0),
            COALESCE(c.supports_vision, 0),
            COALESCE(c.supports_reasoning, 0),
            COALESCE(c.model_allowlist_json, '[]'),
            COALESCE(c.model_blocklist_json, '[]'),
            h.cooldown_until,
            COALESCE(bs.status, ''),
            k.max_concurrency,
            k.load_factor
         FROM station_keys k
         JOIN stations s ON s.id = k.station_id
         LEFT JOIN station_group_bindings b ON b.id = k.group_binding_id
         LEFT JOIN station_group_bindings cb ON cb.id = (
             SELECT canonical.id
               FROM station_group_bindings canonical
              WHERE canonical.station_id = k.station_id
                AND canonical.binding_kind = 'station_group'
                AND canonical.binding_status = 'available'
                AND COALESCE(canonical.rate_source, '') != 'legacy_key_group'
                AND lower(trim(canonical.group_name)) = lower(trim(COALESCE(b.group_name, k.group_name)))
                AND b.id IS NOT NULL
                AND (
                     b.binding_kind != 'station_group'
                     OR b.binding_status != 'available'
                     OR COALESCE(b.rate_source, '') = 'legacy_key_group'
                )
              ORDER BY canonical.updated_at DESC, canonical.created_at DESC, canonical.id ASC
              LIMIT 1
         )
         LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
         LEFT JOIN station_key_health h ON h.station_key_id = k.id
         LEFT JOIN balance_snapshots bs ON bs.id = (
             SELECT latest_balance.id
               FROM balance_snapshots latest_balance
              WHERE latest_balance.station_key_id = k.id
                 OR (
                    latest_balance.station_key_id IS NULL
                    AND latest_balance.station_id = k.station_id
                    AND latest_balance.scope = 'station'
                 )
              ORDER BY latest_balance.updated_at DESC, latest_balance.created_at DESC
              LIMIT 1
         )",
    );
    let mut where_clauses =
        vec!["(TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)".to_string()];
    let mut sql_params = Vec::new();
    let mut group_type_filter = None;
    match filter {
        RoutingGroupFilter::AllGroups => {}
        RoutingGroupFilter::UngroupedOnly => {
            where_clauses
                .push("k.group_binding_id IS NULL AND k.group_id_hash IS NULL".to_string());
        }
        RoutingGroupFilter::GroupBindingId(group_binding_id) => {
            where_clauses.push("COALESCE(cb.id, k.group_binding_id) = ?".to_string());
            sql_params.push(group_binding_id.clone());
        }
        RoutingGroupFilter::GroupIdHash(group_id_hash) => {
            where_clauses.push(
                "COALESCE(cb.group_id_hash, b.group_id_hash, k.group_id_hash) = ?".to_string(),
            );
            sql_params.push(group_id_hash.clone());
        }
        RoutingGroupFilter::GroupType(group_type) => {
            group_type_filter = Some(group_type.clone());
        }
    }
    sql.push_str(" WHERE ");
    sql.push_str(&where_clauses.join(" AND "));
    sql.push_str(" ORDER BY COALESCE(k.routing_order, k.priority) ASC, k.priority ASC, k.created_at ASC, k.id ASC");

    let rows = {
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("read scheduler candidates failed: {error}"))?;
        let rows = statement
            .query_map(params_from_iter(sql_params.iter()), |row| {
                let cooldown_until: Option<String> = row.get(20)?;
                let balance_status: String = row.get(21)?;
                Ok(SchedulerCandidateBaseRow {
                    station_key_id: row.get(0)?,
                    station_id: row.get(1)?,
                    priority: row.get(2)?,
                    max_concurrency: row.get(22)?,
                    load_factor: row.get(23)?,
                    group_binding_id: row.get(3)?,
                    group_id_hash: row.get(4)?,
                    group_name: row.get(5)?,
                    inferred_group_category: row.get(6)?,
                    group_category_override: row.get(7)?,
                    station_enabled: i64_to_bool(row.get(8)?),
                    key_enabled: i64_to_bool(row.get(9)?),
                    schedulable: i64_to_bool(row.get(10)?),
                    supports_chat_completions: i64_to_bool(row.get(11)?),
                    supports_responses: i64_to_bool(row.get(12)?),
                    supports_embeddings: i64_to_bool(row.get(13)?),
                    supports_stream: i64_to_bool(row.get(14)?),
                    supports_tools: i64_to_bool(row.get(15)?),
                    supports_vision: i64_to_bool(row.get(16)?),
                    supports_reasoning: i64_to_bool(row.get(17)?),
                    model_allowlist: parse_json_string_list(row.get::<_, String>(18)?.as_str()),
                    model_blocklist: parse_json_string_list(row.get::<_, String>(19)?.as_str()),
                    health_blocked: cooldown_until
                        .as_deref()
                        .and_then(|value| parse_optional_millisecond_timestamp(Some(value)))
                        .is_some_and(|cooldown_until_ms| now_ms < cooldown_until_ms),
                    balance_depleted: matches!(
                        balance_status.trim().to_ascii_lowercase().as_str(),
                        "depleted" | "insufficient" | "blocked"
                    ),
                })
            })
            .map_err(|error| format!("query scheduler candidates failed: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("parse scheduler candidates failed: {error}"))?;
        rows
    };

    rows.into_iter()
        .filter(|row| {
            group_type_filter
                .as_ref()
                .map(|expected| scheduler_row_group_type(row).as_ref() == Some(expected))
                .unwrap_or(true)
        })
        .map(|row| {
            let group_type = scheduler_row_group_type(&row);
            let multiplier_facts =
                load_multiplier_source_facts_from_connection(connection, &row.station_key_id)?;
            let multiplier_result = resolve_effective_multiplier(
                multiplier_facts,
                now_ms,
                advanced_settings.multiplier_min_confidence,
                group_rate_interval_ms,
            );
            let (effective_multiplier, multiplier_reject_reason) = match multiplier_result {
                Ok(fact) => (Some(fact), None),
                Err(reason) => (None, Some(reason)),
            };
            Ok(SchedulerCandidate {
                station_key_id: row.station_key_id,
                station_id: row.station_id,
                priority: row.priority,
                max_concurrency: row.max_concurrency,
                load_factor: row.load_factor,
                group_binding_id: row.group_binding_id,
                group_id_hash: row.group_id_hash,
                group_type,
                station_enabled: row.station_enabled,
                key_enabled: row.key_enabled,
                schedulable: row.schedulable,
                supports_chat_completions: row.supports_chat_completions,
                supports_responses: row.supports_responses,
                supports_embeddings: row.supports_embeddings,
                supports_stream: row.supports_stream,
                supports_tools: row.supports_tools,
                supports_vision: row.supports_vision,
                supports_reasoning: row.supports_reasoning,
                model_allowlist: row.model_allowlist,
                model_blocklist: row.model_blocklist,
                health_blocked: row.health_blocked,
                balance_depleted: row.balance_depleted,
                effective_multiplier,
                multiplier_reject_reason,
            })
        })
        .collect()
}

fn parse_pricing_group_type(value: Option<&str>) -> Option<PricingGroupType> {
    let normalized = normalize_group_type_text(value?);
    if text_matches_any_group_type_matcher(
        &normalized,
        &[
            "图",
            "画图",
            "绘图",
            "image",
            "images",
            "picture",
            "pictures",
            "dall-e",
            "midjourney",
        ],
    ) {
        return Some(PricingGroupType::ImageGeneration);
    }
    if text_matches_any_group_type_matcher(
        &normalized,
        &[
            "claude",
            "anthropic",
            "sonnet",
            "opus",
            "haiku",
            "yellow",
            "amber",
        ],
    ) {
        return Some(PricingGroupType::Claude);
    }
    if text_matches_any_group_type_matcher(&normalized, &["gemini", "google"]) {
        return Some(PricingGroupType::Gemini);
    }
    if text_matches_any_group_type_matcher(&normalized, &["grok", "xai", "x-ai"]) {
        return Some(PricingGroupType::Grok);
    }
    if text_matches_any_group_type_matcher(
        &normalized,
        &["openai", "gpt", "codex", "default", "green"],
    ) {
        return Some(PricingGroupType::Gpt);
    }
    None
}

fn pricing_group_type_from_binding_metadata(
    group_category_override: Option<&str>,
    inferred_group_category: Option<&str>,
    group_name: Option<&str>,
) -> Option<PricingGroupType> {
    group_category_override
        .and_then(|value| parse_pricing_group_type(Some(value)))
        .or_else(|| inferred_group_category.and_then(|value| parse_pricing_group_type(Some(value))))
        .or_else(|| parse_pricing_group_type(group_name))
}

fn text_matches_any_group_type_matcher(value: &str, matchers: &[&str]) -> bool {
    matchers
        .iter()
        .map(|matcher| normalize_group_type_text(matcher))
        .filter(|matcher| !matcher.is_empty())
        .any(|matcher| value.contains(&matcher))
}

fn normalize_group_type_text(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character == '_' || character.is_whitespace() {
                '-'
            } else {
                character
            }
        })
        .collect()
}

fn list_remote_station_keys_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
                    api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
                    rate_source, created_at_remote, last_used_at, raw_source, match_status,
                    matched_station_key_id, match_confidence, collected_at
               FROM remote_station_keys
              WHERE station_id = ?1
              ORDER BY collected_at DESC, id ASC",
        )
        .map_err(|error| format!("读取远端 Key 发现列表失败: {error}"))?;

    let rows = statement
        .query_map(params![station_id], row_to_remote_station_key)
        .map_err(|error| format!("查询远端 Key 发现失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析远端 Key 发现失败: {error}"))?;
    Ok(rows)
}

fn row_to_remote_station_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteStationKey> {
    let match_status: String = row.get(14)?;
    Ok(RemoteStationKey {
        id: row.get(0)?,
        station_id: row.get(1)?,
        remote_key_id_hash: row.get(2)?,
        remote_key_name: row.get(3)?,
        api_key_masked: row.get(4)?,
        api_key_fingerprint: row.get(5)?,
        group_id_hash: row.get(6)?,
        group_name: row.get(7)?,
        tier_label: row.get(8)?,
        rate_multiplier: row.get(9)?,
        rate_source: row.get(10)?,
        created_at: row.get(11)?,
        last_used_at: row.get(12)?,
        raw_source: row.get(13)?,
        match_status: RemoteKeyMatchStatus::from_str(&match_status),
        matched_station_key_id: row.get(15)?,
        match_confidence: row.get(16)?,
        collected_at: row.get(17)?,
    })
}

fn insert_remote_station_key(
    transaction: &rusqlite::Transaction<'_>,
    station_id: &str,
    key: &RemoteStationKey,
) -> Result<(), String> {
    let matched_station_key_id = valid_remote_station_key_match(
        transaction,
        station_id,
        key.matched_station_key_id.as_deref(),
    )
    .map_err(|error| format!("校验远端 Key 匹配关系失败: {error}"))?;
    let (match_status, match_confidence) =
        if key.matched_station_key_id.is_some() && matched_station_key_id.is_none() {
            (RemoteKeyMatchStatus::Unbound.as_str(), 0.0)
        } else {
            (key.match_status.as_str(), key.match_confidence)
        };

    transaction
        .execute(
            "INSERT INTO remote_station_keys (
                id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
                api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
                rate_source, created_at_remote, last_used_at, raw_source, match_status,
                matched_station_key_id, match_confidence, collected_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                key.id,
                station_id,
                key.remote_key_id_hash,
                key.remote_key_name,
                key.api_key_masked,
                key.api_key_fingerprint,
                key.group_id_hash,
                key.group_name,
                key.tier_label,
                key.rate_multiplier,
                key.rate_source,
                key.created_at,
                key.last_used_at,
                key.raw_source,
                match_status,
                matched_station_key_id,
                match_confidence,
                key.collected_at,
                now_string(),
            ],
        )
        .map_err(|error| format!("写入远端 Key 发现失败: {error}"))?;
    Ok(())
}

fn valid_remote_station_key_match(
    connection: &Connection,
    station_id: &str,
    matched_station_key_id: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    let Some(matched_station_key_id) = matched_station_key_id else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT id FROM station_keys WHERE id = ?1 AND station_id = ?2",
            params![matched_station_key_id, station_id],
            |row| row.get(0),
        )
        .optional()
}

fn list_key_pool_items_from_connection(
    connection: &Connection,
) -> Result<Vec<KeyPoolItem>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                k.id,
                k.station_id,
                s.name,
                s.station_type,
                s.api_base_url,
                s.endpoint_revision,
                k.name,
                k.api_key,
                (SELECT masked_value FROM secrets WHERE secrets.id = k.api_key_secret_id),
                k.api_key_secret_id,
                k.enabled,
                k.priority,
                k.max_concurrency,
                k.load_factor,
                k.schedulable,
                k.group_name,
                k.tier_label,
                k.group_binding_id,
                k.group_id_hash,
                k.rate_multiplier,
                k.manual_rate_multiplier,
                k.manual_rate_updated_at,
                k.rate_source,
                k.rate_collected_at,
                k.balance_scope,
                k.status,
                k.last_checked_at,
                k.last_used_at,
                k.note,
                k.created_at,
                k.updated_at,
                COALESCE(c.supports_chat_completions, 1),
                COALESCE(c.supports_responses, 1),
                COALESCE(c.supports_embeddings, 0),
                COALESCE(c.supports_stream, 1),
                COALESCE(c.supports_tools, 0),
                COALESCE(c.supports_vision, 0),
                COALESCE(c.supports_reasoning, 0),
                COALESCE(c.model_allowlist_json, '[]'),
                COALESCE(c.model_blocklist_json, '[]'),
                COALESCE(c.preferred_models_json, '[]'),
                COALESCE(c.only_use_as_backup, 0),
                h.cooldown_until,
                h.success_count,
                h.failure_count,
                h.avg_latency_ms,
                COALESCE(h.consecutive_failures, 0),
                h.last_error_summary,
                s.upstream_api_format,
                COALESCE(eh.status, 'unchecked') AS endpoint_ping_status,
                eh.latency_ms AS endpoint_ping_ms,
                eh.checked_at AS endpoint_ping_checked_at,
                eh.error_summary AS endpoint_ping_error
             FROM station_keys k
             INNER JOIN stations s ON s.id = k.station_id
             LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
             LEFT JOIN station_key_health h
                    ON h.station_key_id = k.id
                   AND h.endpoint_revision = s.endpoint_revision
             LEFT JOIN station_endpoint_health eh
                    ON eh.station_id = s.id
                   AND eh.endpoint_revision = s.endpoint_revision
             ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                      k.priority ASC,
                      k.created_at ASC,
                      k.id ASC",
        )
        .map_err(|error| format!("读取 Key 池失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let api_key: String = row.get(7)?;
            let secret_masked: Option<String> = row.get(8)?;
            let api_key_secret_id: Option<String> = row.get(9)?;
            let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
            let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();
            let supports_chat = i64_to_bool(row.get(31)?);
            let supports_responses = i64_to_bool(row.get(32)?);
            let supports_embeddings = i64_to_bool(row.get(33)?);
            let supports_stream = i64_to_bool(row.get(34)?);
            let supports_tools = i64_to_bool(row.get(35)?);
            let supports_vision = i64_to_bool(row.get(36)?);
            let supports_reasoning = i64_to_bool(row.get(37)?);
            let allowlist = parse_json_string_list(row.get::<_, String>(38)?.as_str());
            let blocklist = parse_json_string_list(row.get::<_, String>(39)?.as_str());
            let preferred_models = parse_json_string_list(row.get::<_, String>(40)?.as_str());
            let success_count = row.get::<_, Option<i64>>(43)?.unwrap_or(0);
            let failure_count = row.get::<_, Option<i64>>(44)?.unwrap_or(0);
            Ok(KeyPoolItem {
                id: row.get(0)?,
                station_id: row.get(1)?,
                station_name: row.get(2)?,
                station_type: row.get(3)?,
                station_api_base_url: row.get(4)?,
                station_endpoint_revision: row.get(5)?,
                station_upstream_api_format: row.get(48)?,
                name: row.get(6)?,
                api_key_masked,
                api_key_present,
                enabled: i64_to_bool(row.get(10)?),
                priority: row.get(11)?,
                max_concurrency: row.get(12)?,
                load_factor: row.get(13)?,
                schedulable: i64_to_bool(row.get(14)?),
                group_name: row.get(15)?,
                tier_label: row.get(16)?,
                group_binding_id: row.get(17)?,
                group_id_hash: row.get(18)?,
                rate_multiplier: row.get(19)?,
                manual_rate_multiplier: row.get(20)?,
                manual_rate_updated_at: row.get(21)?,
                rate_source: row.get(22)?,
                rate_collected_at: row.get(23)?,
                balance_scope: row.get(24)?,
                status: row.get(25)?,
                last_checked_at: row.get(26)?,
                last_used_at: row.get(27)?,
                note: row.get(28)?,
                capability_summary: summarize_capabilities(
                    supports_chat,
                    supports_responses,
                    supports_embeddings,
                    supports_stream,
                    supports_tools,
                    supports_vision,
                    supports_reasoning,
                ),
                model_scope_summary: summarize_model_scope(
                    allowlist.len(),
                    blocklist.len(),
                    preferred_models.len(),
                ),
                only_use_as_backup: i64_to_bool(row.get(41)?),
                cooldown_until: row.get(42)?,
                success_rate: success_rate(success_count, failure_count),
                avg_latency_ms: row.get(45)?,
                consecutive_failures: row.get(46)?,
                last_error_summary: row.get(47)?,
                endpoint_ping_status: row.get(49)?,
                endpoint_ping_ms: row.get(50)?,
                endpoint_ping_checked_at: row.get(51)?,
                endpoint_ping_error: row.get(52)?,
                created_at: row.get(29)?,
                updated_at: row.get(30)?,
            })
        })
        .map_err(|error| format!("查询 Key 池失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Key 池失败: {error}"))?;

    Ok(rows)
}

pub(super) fn station_key_capabilities_by_id(
    connection: &Connection,
    station_key_id: &str,
) -> Result<StationKeyCapabilities, String> {
    validate_station_key_exists(connection, station_key_id)?;
    let row = connection
        .query_row(
            "SELECT station_key_id, supports_chat_completions, supports_responses,
                    supports_embeddings, supports_stream, supports_tools, supports_vision,
                    supports_reasoning, model_allowlist_json, model_blocklist_json,
                    preferred_models_json, only_use_as_backup, routing_tags_json, updated_at
               FROM station_key_capabilities
              WHERE station_key_id = ?1",
            params![station_key_id],
            row_to_station_key_capabilities,
        )
        .optional()
        .map_err(|error| format!("读取 Key 能力配置失败: {error}"))?;

    Ok(row.unwrap_or_else(|| default_station_key_capabilities(station_key_id)))
}

pub(super) fn update_station_key_capabilities_in_connection(
    connection: &Connection,
    input: UpdateStationKeyCapabilitiesInput,
) -> Result<StationKeyCapabilities, String> {
    validate_station_key_exists(connection, &input.station_key_id)?;
    let now = now_string();
    connection
        .execute(
            "INSERT INTO station_key_capabilities (
                station_key_id, supports_chat_completions, supports_responses,
                supports_embeddings, supports_stream, supports_tools, supports_vision,
                supports_reasoning, model_allowlist_json, model_blocklist_json,
                preferred_models_json, only_use_as_backup, routing_tags_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(station_key_id) DO UPDATE SET
                supports_chat_completions = excluded.supports_chat_completions,
                supports_responses = excluded.supports_responses,
                supports_embeddings = excluded.supports_embeddings,
                supports_stream = excluded.supports_stream,
                supports_tools = excluded.supports_tools,
                supports_vision = excluded.supports_vision,
                supports_reasoning = excluded.supports_reasoning,
                model_allowlist_json = excluded.model_allowlist_json,
                model_blocklist_json = excluded.model_blocklist_json,
                preferred_models_json = excluded.preferred_models_json,
                only_use_as_backup = excluded.only_use_as_backup,
                routing_tags_json = excluded.routing_tags_json,
                updated_at = excluded.updated_at",
            params![
                input.station_key_id,
                bool_to_i64(input.supports_chat_completions),
                bool_to_i64(input.supports_responses),
                bool_to_i64(input.supports_embeddings),
                bool_to_i64(input.supports_stream),
                bool_to_i64(input.supports_tools),
                bool_to_i64(input.supports_vision),
                bool_to_i64(input.supports_reasoning),
                serialize_string_list(&input.model_allowlist)?,
                serialize_string_list(&input.model_blocklist)?,
                serialize_string_list(&input.preferred_models)?,
                bool_to_i64(input.only_use_as_backup),
                serialize_string_list(&input.routing_tags)?,
                now,
            ],
        )
        .map_err(|error| format!("保存 Key 能力配置失败: {error}"))?;

    station_key_capabilities_by_id(connection, &input.station_key_id)
}

fn row_to_station_key_capabilities(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StationKeyCapabilities> {
    Ok(StationKeyCapabilities {
        station_key_id: row.get(0)?,
        supports_chat_completions: i64_to_bool(row.get(1)?),
        supports_responses: i64_to_bool(row.get(2)?),
        supports_embeddings: i64_to_bool(row.get(3)?),
        supports_stream: i64_to_bool(row.get(4)?),
        supports_tools: i64_to_bool(row.get(5)?),
        supports_vision: i64_to_bool(row.get(6)?),
        supports_reasoning: i64_to_bool(row.get(7)?),
        model_allowlist: parse_json_string_list(row.get::<_, String>(8)?.as_str()),
        model_blocklist: parse_json_string_list(row.get::<_, String>(9)?.as_str()),
        preferred_models: parse_json_string_list(row.get::<_, String>(10)?.as_str()),
        only_use_as_backup: i64_to_bool(row.get(11)?),
        routing_tags: parse_json_string_list(row.get::<_, String>(12)?.as_str()),
        updated_at: row.get(13)?,
    })
}

fn default_station_key_capabilities(station_key_id: &str) -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: station_key_id.to_string(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: false,
        supports_stream: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        model_allowlist: Vec::new(),
        model_blocklist: Vec::new(),
        preferred_models: Vec::new(),
        only_use_as_backup: false,
        routing_tags: Vec::new(),
        updated_at: now_string(),
    }
}

fn list_model_aliases_from_connection(connection: &Connection) -> Result<Vec<ModelAlias>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, client_model, upstream_model, enabled, note, created_at, updated_at
               FROM model_aliases
              ORDER BY client_model ASC, upstream_model ASC",
        )
        .map_err(|error| format!("读取模型映射列表失败: {error}"))?;

    let rows = statement
        .query_map([], row_to_model_alias)
        .map_err(|error| format!("查询模型映射失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析模型映射失败: {error}"))?;

    Ok(rows)
}

fn upsert_model_alias_in_connection(
    connection: &Connection,
    input: UpsertModelAliasInput,
) -> Result<ModelAlias, String> {
    let client_model = input.client_model.trim();
    let upstream_model = input.upstream_model.trim();
    if client_model.is_empty() {
        return Err("客户端模型名不能为空".to_string());
    }
    if upstream_model.is_empty() {
        return Err("上游模型名不能为空".to_string());
    }

    let id = input.id.unwrap_or_else(|| generate_id("alias"));
    let now = now_string();
    connection
        .execute(
            "INSERT INTO model_aliases (
                id, client_model, upstream_model, enabled, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(client_model, upstream_model) DO UPDATE SET
                enabled = excluded.enabled,
                note = excluded.note,
                updated_at = excluded.updated_at",
            params![
                id,
                client_model,
                upstream_model,
                bool_to_i64(input.enabled),
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存模型映射失败: {error}"))?;

    model_alias_by_pair(connection, client_model, upstream_model)
}

fn delete_model_alias_in_connection(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM model_aliases WHERE id = ?1", params![id])
        .map_err(|error| format!("删除模型映射失败: {error}"))?;
    Ok(())
}

fn model_alias_by_pair(
    connection: &Connection,
    client_model: &str,
    upstream_model: &str,
) -> Result<ModelAlias, String> {
    connection
        .query_row(
            "SELECT id, client_model, upstream_model, enabled, note, created_at, updated_at
               FROM model_aliases
              WHERE client_model = ?1 AND upstream_model = ?2",
            params![client_model, upstream_model],
            row_to_model_alias,
        )
        .optional()
        .map_err(|error| format!("读取模型映射失败: {error}"))?
        .ok_or_else(|| "模型映射不存在".to_string())
}

fn row_to_model_alias(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelAlias> {
    Ok(ModelAlias {
        id: row.get(0)?,
        client_model: row.get(1)?,
        upstream_model: row.get(2)?,
        enabled: i64_to_bool(row.get(3)?),
        note: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn list_station_key_health_from_connection(
    connection: &Connection,
) -> Result<Vec<StationKeyHealth>, String> {
    let mut statement = connection
        .prepare(
            "SELECT h.station_key_id, h.last_success_at, h.last_failure_at, h.consecutive_failures,
                    h.success_count, h.failure_count, h.avg_latency_ms, h.last_error_summary,
                    h.cooldown_until, h.updated_at
               FROM station_key_health h
               JOIN station_keys k ON k.id = h.station_key_id
               JOIN stations s ON s.id = k.station_id
              WHERE h.endpoint_revision = s.endpoint_revision
              ORDER BY h.updated_at DESC",
        )
        .map_err(|error| format!("读取 Key 健康状态失败: {error}"))?;

    let rows = statement
        .query_map([], row_to_station_key_health)
        .map_err(|error| format!("查询 Key 健康状态失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Key 健康状态失败: {error}"))?;

    Ok(rows)
}

fn station_key_health_by_id(
    connection: &Connection,
    station_key_id: &str,
) -> Result<StationKeyHealth, String> {
    validate_station_key_exists(connection, station_key_id)?;
    let row = connection
        .query_row(
            "SELECT h.station_key_id, h.last_success_at, h.last_failure_at, h.consecutive_failures,
                    h.success_count, h.failure_count, h.avg_latency_ms, h.last_error_summary,
                    h.cooldown_until, h.updated_at
               FROM station_key_health h
               JOIN station_keys k ON k.id = h.station_key_id
               JOIN stations s ON s.id = k.station_id
              WHERE h.station_key_id = ?1
                AND h.endpoint_revision = s.endpoint_revision",
            params![station_key_id],
            row_to_station_key_health,
        )
        .optional()
        .map_err(|error| format!("读取 Key 健康状态失败: {error}"))?;

    Ok(row.unwrap_or_else(|| default_station_key_health(station_key_id)))
}

fn row_to_station_key_health(row: &rusqlite::Row<'_>) -> rusqlite::Result<StationKeyHealth> {
    Ok(StationKeyHealth {
        station_key_id: row.get(0)?,
        last_success_at: row.get(1)?,
        last_failure_at: row.get(2)?,
        consecutive_failures: row.get(3)?,
        success_count: row.get(4)?,
        failure_count: row.get(5)?,
        avg_latency_ms: row.get(6)?,
        last_error_summary: row.get(7)?,
        cooldown_until: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn default_station_key_health(station_key_id: &str) -> StationKeyHealth {
    StationKeyHealth {
        station_key_id: station_key_id.to_string(),
        last_success_at: None,
        last_failure_at: None,
        consecutive_failures: 0,
        success_count: 0,
        failure_count: 0,
        avg_latency_ms: None,
        last_error_summary: None,
        cooldown_until: None,
        updated_at: now_string(),
    }
}

fn list_station_endpoint_health_from_connection(
    connection: &Connection,
) -> Result<Vec<StationEndpointHealth>, String> {
    let mut statement = connection
        .prepare(
            "SELECT h.station_id, h.endpoint_revision, h.status, h.latency_ms,
                    h.checked_at, h.error_summary, h.updated_at
               FROM station_endpoint_health h
               JOIN stations s ON s.id = h.station_id
              WHERE h.endpoint_revision = s.endpoint_revision
              ORDER BY h.updated_at DESC",
        )
        .map_err(|error| format!("读取端点 PING 状态失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_station_endpoint_health)
        .map_err(|error| format!("查询端点 PING 状态失败: {error}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| format!("解析端点 PING 状态失败: {error}"))?;
    Ok(rows)
}

fn station_endpoint_health_by_id(
    connection: &Connection,
    station_id: &str,
) -> Result<StationEndpointHealth, String> {
    let endpoint_revision = station_endpoint_revision(connection, station_id)?;
    let row = connection
        .query_row(
            "SELECT h.station_id, h.endpoint_revision, h.status, h.latency_ms,
                    h.checked_at, h.error_summary, h.updated_at
               FROM station_endpoint_health h
              WHERE h.station_id = ?1
                AND h.endpoint_revision = ?2",
            params![station_id, endpoint_revision],
            row_to_station_endpoint_health,
        )
        .optional()
        .map_err(|error| format!("读取端点 PING 状态失败: {error}"))?;
    Ok(row.unwrap_or_else(|| default_station_endpoint_health(station_id, endpoint_revision)))
}

fn row_to_station_endpoint_health(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StationEndpointHealth> {
    Ok(StationEndpointHealth {
        station_id: row.get(0)?,
        endpoint_revision: row.get(1)?,
        status: row.get(2)?,
        latency_ms: row.get(3)?,
        checked_at: row.get(4)?,
        error_summary: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn default_station_endpoint_health(
    station_id: &str,
    endpoint_revision: i64,
) -> StationEndpointHealth {
    StationEndpointHealth {
        station_id: station_id.to_string(),
        endpoint_revision,
        status: "unchecked".to_string(),
        latency_ms: None,
        checked_at: None,
        error_summary: None,
        updated_at: now_string(),
    }
}

fn upsert_station_endpoint_health_in_connection(
    connection: &Connection,
    station_id: &str,
    status: &str,
    latency_ms: Option<i64>,
    checked_at: &str,
    error_summary: Option<&str>,
) -> Result<StationEndpointHealth, String> {
    let endpoint_revision = station_endpoint_revision(connection, station_id)?;
    if !matches!(status, "unchecked" | "success" | "failed") {
        return Err("端点 PING 状态无效".to_string());
    }
    if latency_ms.is_some_and(|value| value < 0) {
        return Err("端点 PING 延迟不能为负数".to_string());
    }

    let updated_at = now_string();
    connection
        .execute(
            "INSERT INTO station_endpoint_health (
                station_id, endpoint_revision, status, latency_ms, checked_at, error_summary, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(station_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                status = excluded.status,
                latency_ms = excluded.latency_ms,
                checked_at = excluded.checked_at,
                error_summary = excluded.error_summary,
                updated_at = excluded.updated_at",
            params![
                station_id,
                endpoint_revision,
                status,
                latency_ms,
                checked_at,
                error_summary,
                updated_at,
            ],
        )
        .map_err(|error| format!("写入端点 PING 状态失败: {error}"))?;
    station_endpoint_health_by_id(connection, station_id)
}

fn station_id_for_key(
    connection: &Connection,
    station_key_id: &str,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取 Key 所属站点失败: {error}"))
}

fn record_station_key_success_in_connection(
    connection: &Connection,
    station_key_id: &str,
    duration_ms: i64,
    now: &str,
) -> Result<(), String> {
    validate_station_key_exists(connection, station_key_id)?;
    let current = station_key_health_by_id(connection, station_key_id)?;
    let endpoint_revision = station_key_endpoint_revision(connection, station_key_id)?;
    let success_count = current.success_count + 1;
    let total_duration_ms = current
        .avg_latency_ms
        .map(|avg| avg * current.success_count)
        .unwrap_or(0)
        + duration_ms.max(0);
    let avg_latency_ms = if success_count > 0 {
        Some(total_duration_ms / success_count)
    } else {
        None
    };

    connection
        .execute(
            "INSERT INTO station_key_health (
                station_key_id, endpoint_revision, last_success_at, last_failure_at, consecutive_failures,
                success_count, failure_count, total_duration_ms, avg_latency_ms,
                last_error_summary, cooldown_until, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, NULL, NULL, ?9)
             ON CONFLICT(station_key_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                last_success_at = excluded.last_success_at,
                last_failure_at = excluded.last_failure_at,
                consecutive_failures = 0,
                success_count = excluded.success_count,
                failure_count = excluded.failure_count,
                total_duration_ms = excluded.total_duration_ms,
                avg_latency_ms = excluded.avg_latency_ms,
                last_error_summary = NULL,
                cooldown_until = NULL,
                updated_at = excluded.updated_at",
            params![
                station_key_id,
                endpoint_revision,
                now,
                current.last_failure_at,
                success_count,
                current.failure_count,
                total_duration_ms,
                avg_latency_ms,
                now,
            ],
        )
        .map_err(|error| format!("记录 Key 成功状态失败: {error}"))?;

    Ok(())
}

fn record_station_key_failure_in_connection(
    connection: &Connection,
    station_key_id: &str,
    error_summary: &str,
    now: &str,
    consecutive_failure_threshold: i64,
) -> Result<(), String> {
    let current = station_key_health_by_id(connection, station_key_id)?;
    let consecutive_failures = current.consecutive_failures + 1;
    let cooldown_until =
        cooldown_until_with_threshold(consecutive_failures, consecutive_failure_threshold, now);

    record_station_key_failure_state_in_connection(
        connection,
        station_key_id,
        error_summary,
        now,
        &current,
        consecutive_failures,
        cooldown_until.as_deref(),
    )
}

fn record_station_key_failure_with_explicit_cooldown_in_connection(
    connection: &Connection,
    station_key_id: &str,
    error_summary: &str,
    now: &str,
    cooldown_until: Option<&str>,
) -> Result<(), String> {
    let current = station_key_health_by_id(connection, station_key_id)?;
    let consecutive_failures = current.consecutive_failures + 1;

    record_station_key_failure_state_in_connection(
        connection,
        station_key_id,
        error_summary,
        now,
        &current,
        consecutive_failures,
        cooldown_until,
    )
}

fn record_station_key_failure_state_in_connection(
    connection: &Connection,
    station_key_id: &str,
    error_summary: &str,
    now: &str,
    current: &StationKeyHealth,
    consecutive_failures: i64,
    cooldown_until: Option<&str>,
) -> Result<(), String> {
    validate_station_key_exists(connection, station_key_id)?;
    let endpoint_revision = station_key_endpoint_revision(connection, station_key_id)?;
    let failure_count = current.failure_count + 1;
    let cooldown_until = cooldown_until.map(ToString::to_string);

    connection
        .execute(
            "INSERT INTO station_key_health (
                station_key_id, endpoint_revision, last_success_at, last_failure_at, consecutive_failures,
                success_count, failure_count, total_duration_ms, avg_latency_ms,
                last_error_summary, cooldown_until, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(station_key_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                last_success_at = excluded.last_success_at,
                last_failure_at = excluded.last_failure_at,
                consecutive_failures = excluded.consecutive_failures,
                success_count = excluded.success_count,
                failure_count = excluded.failure_count,
                total_duration_ms = excluded.total_duration_ms,
                avg_latency_ms = excluded.avg_latency_ms,
                last_error_summary = excluded.last_error_summary,
                cooldown_until = excluded.cooldown_until,
                updated_at = excluded.updated_at",
            params![
                station_key_id,
                endpoint_revision,
                current.last_success_at,
                now,
                consecutive_failures,
                current.success_count,
                failure_count,
                current
                    .avg_latency_ms
                    .map(|avg| avg * current.success_count)
                    .unwrap_or(0),
                current.avg_latency_ms,
                trim_error_summary(error_summary),
                cooldown_until.clone(),
                now,
            ],
        )
        .map_err(|error| format!("记录 Key 失败状态失败: {error}"))?;

    if let Some(station_id) = station_id_for_key(connection, station_key_id)? {
        let station_key = station_key_by_id(connection, station_key_id).ok();
        if let Some(event) = crate::services::change_events::key_health_event(
            station_key_id,
            &station_id,
            station_key.as_ref().map(|key| key.name.as_str()),
            station_key.as_ref().map(|key| key.api_key_masked.as_str()),
            consecutive_failures,
            Some(&trim_error_summary(error_summary)),
            cooldown_until.as_deref(),
        ) {
            let _ = upsert_change_event_in_connection(connection, event);
        }
    }

    Ok(())
}

fn cooldown_until_with_threshold(
    consecutive_failures: i64,
    consecutive_failure_threshold: i64,
    now: &str,
) -> Option<String> {
    let now = now.parse::<i64>().ok()?;
    let threshold = consecutive_failure_threshold.max(1);
    let duration_ms = match consecutive_failures - threshold {
        failures_before_threshold if failures_before_threshold < 0 => return None,
        0 => 2 * 60 * 1000,
        1 => 5 * 60 * 1000,
        _ => 15 * 60 * 1000,
    };
    Some((now + duration_ms).to_string())
}

fn trim_error_summary(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 160 {
        return trimmed.to_string();
    }
    let boundary = trimmed
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= 160)
        .last()
        .unwrap_or(0);
    let mut output = trimmed.to_string();
    output.truncate(boundary);
    output.push_str("...");
    output
}

fn proxy_route_candidates_from_connection(
    connection: &Connection,
) -> Result<Vec<RouteCandidate>, String> {
    proxy_route_candidates_from_connection_with_data_key(connection, None)
}

fn proxy_route_candidates_from_connection_with_data_key(
    connection: &Connection,
    data_key: Option<&[u8; 32]>,
) -> Result<Vec<RouteCandidate>, String> {
    let global_proxy_mode = read_setting_or_default(connection, "collector_proxy_mode", "direct")?;
    let global_proxy_url = read_setting_or_default(connection, "collector_proxy_url", "")?;
    let mut statement = connection
        .prepare(
            "SELECT k.id, k.station_id, s.endpoint_revision, s.api_base_url, k.api_key, k.api_key_secret_id,
                    s.upstream_api_format, COALESCE(k.routing_order, k.priority),
                    s.collector_proxy_mode, s.collector_proxy_url,
                    k.max_concurrency, k.load_factor, k.schedulable
               FROM station_keys k
               JOIN stations s ON s.id = k.station_id
              WHERE k.enabled = 1
                AND s.enabled = 1
                AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
              ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                       k.priority ASC,
                       k.created_at ASC,
                       k.id ASC",
        )
        .map_err(|error| format!("读取 Key 池候选失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let station_key_id: String = row.get(0)?;
            let api_key: String = row.get(4)?;
            Ok(RouteCandidate {
                station_key_id,
                station_id: row.get(1)?,
                station_endpoint_revision: row.get(2)?,
                upstream_base_url: row.get(3)?,
                api_key,
                upstream_api_format: parse_upstream_api_format(row.get::<_, String>(6)?),
                priority: row.get(7)?,
                max_concurrency: row.get(10)?,
                load_factor: row.get(11)?,
                schedulable: i64_to_bool(row.get(12)?),
                collector_proxy_mode: {
                    let proxy = resolve_proxy_config(
                        &row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        &global_proxy_mode,
                        Some(global_proxy_url.clone()),
                    );
                    proxy.mode
                },
                collector_proxy_url: {
                    let proxy = resolve_proxy_config(
                        &row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        &global_proxy_mode,
                        Some(global_proxy_url.clone()),
                    );
                    proxy.url
                },
            })
        })
        .map_err(|error| format!("查询 Key 池候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Key 池候选失败: {error}"))?;

    rows.into_iter()
        .map(|candidate| {
            if candidate.api_key.trim().is_empty() {
                let Some(data_key) = data_key else {
                    return Err("Station Key 已迁移为加密凭据，当前调用缺少解密密钥".to_string());
                };
                Ok(RouteCandidate {
                    api_key: resolve_station_key_api_key(
                        connection,
                        data_key,
                        &candidate.station_key_id,
                    )?,
                    ..candidate
                })
            } else {
                Ok(candidate)
            }
        })
        .collect()
}

fn proxy_rich_route_candidates_from_connection(
    connection: &Connection,
) -> Result<Vec<RichRouteCandidate>, String> {
    proxy_rich_route_candidates_from_connection_with_data_key(connection, None)
}

fn proxy_rich_route_candidates_from_connection_with_data_key(
    connection: &Connection,
    data_key: Option<&[u8; 32]>,
) -> Result<Vec<RichRouteCandidate>, String> {
    let global_proxy_mode = read_setting_or_default(connection, "collector_proxy_mode", "direct")?;
    let global_proxy_url = read_setting_or_default(connection, "collector_proxy_url", "")?;
    let mut statement = connection
        .prepare(
            "SELECT
                k.id,
                k.station_id,
                s.endpoint_revision,
                s.api_base_url,
                k.api_key,
                k.api_key_secret_id,
                s.upstream_api_format,
                COALESCE(k.routing_order, k.priority),
                s.name,
                k.name,
                COALESCE(c.supports_chat_completions, 1),
                COALESCE(c.supports_responses, 1),
                COALESCE(c.supports_embeddings, 0),
                COALESCE(c.supports_stream, 1),
                COALESCE(c.supports_tools, 0),
                COALESCE(c.supports_vision, 0),
                COALESCE(c.supports_reasoning, 0),
                COALESCE(c.model_allowlist_json, '[]'),
                COALESCE(c.model_blocklist_json, '[]'),
                COALESCE(c.preferred_models_json, '[]'),
                COALESCE(c.only_use_as_backup, 0),
                COALESCE(c.routing_tags_json, '[]'),
                COALESCE(c.updated_at, '0'),
                k.schedulable,
                h.station_key_id,
                h.last_success_at,
                h.last_failure_at,
                h.consecutive_failures,
                h.success_count,
                h.failure_count,
                h.avg_latency_ms,
                h.last_error_summary,
                h.cooldown_until,
                h.updated_at,
                s.collector_proxy_mode,
                s.collector_proxy_url,
                k.max_concurrency,
                k.load_factor
             FROM station_keys k
             JOIN stations s ON s.id = k.station_id
             LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
             LEFT JOIN station_key_health h
                    ON h.station_key_id = k.id
                   AND h.endpoint_revision = s.endpoint_revision
             WHERE k.enabled = 1
               AND s.enabled = 1
               AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
             ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                      k.priority ASC,
                      k.created_at ASC,
                      k.id ASC",
        )
        .map_err(|error| format!("读取富路由候选失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let station_key_id = row.get::<_, String>(0)?;
            let api_key: String = row.get(4)?;
            let health_station_key_id = row.get::<_, Option<String>>(24)?;
            let proxy = resolve_proxy_config(
                &row.get::<_, String>(34)?,
                row.get::<_, Option<String>>(35)?,
                &global_proxy_mode,
                Some(global_proxy_url.clone()),
            );
            Ok(RichRouteCandidate {
                candidate: RouteCandidate {
                    station_key_id: station_key_id.clone(),
                    station_id: row.get(1)?,
                    station_endpoint_revision: row.get(2)?,
                    upstream_base_url: row.get(3)?,
                    api_key,
                    upstream_api_format: parse_upstream_api_format(row.get::<_, String>(6)?),
                    priority: row.get(7)?,
                    max_concurrency: row.get(36)?,
                    load_factor: row.get(37)?,
                    schedulable: i64_to_bool(row.get(23)?),
                    collector_proxy_mode: proxy.mode,
                    collector_proxy_url: proxy.url,
                },
                station_name: row.get(8)?,
                key_name: row.get(9)?,
                capabilities: StationKeyCapabilities {
                    station_key_id,
                    supports_chat_completions: i64_to_bool(row.get(10)?),
                    supports_responses: i64_to_bool(row.get(11)?),
                    supports_embeddings: i64_to_bool(row.get(12)?),
                    supports_stream: i64_to_bool(row.get(13)?),
                    supports_tools: i64_to_bool(row.get(14)?),
                    supports_vision: i64_to_bool(row.get(15)?),
                    supports_reasoning: i64_to_bool(row.get(16)?),
                    model_allowlist: parse_json_string_list(row.get::<_, String>(17)?.as_str()),
                    model_blocklist: parse_json_string_list(row.get::<_, String>(18)?.as_str()),
                    preferred_models: parse_json_string_list(row.get::<_, String>(19)?.as_str()),
                    only_use_as_backup: i64_to_bool(row.get(20)?),
                    routing_tags: parse_json_string_list(row.get::<_, String>(21)?.as_str()),
                    updated_at: row.get(22)?,
                },
                health: health_station_key_id.map(|station_key_id| StationKeyHealth {
                    station_key_id,
                    last_success_at: row.get(25).ok().flatten(),
                    last_failure_at: row.get(26).ok().flatten(),
                    consecutive_failures: row.get(27).unwrap_or(0),
                    success_count: row.get(28).unwrap_or(0),
                    failure_count: row.get(29).unwrap_or(0),
                    avg_latency_ms: row.get(30).ok().flatten(),
                    last_error_summary: row.get(31).ok().flatten(),
                    cooldown_until: row.get(32).ok().flatten(),
                    updated_at: row.get(33).unwrap_or_else(|_| "0".to_string()),
                }),
                economics: None,
                scheduler_group_binding_id: None,
                scheduler_group_id_hash: None,
                scheduler_group_type: None,
                scheduler_effective_multiplier: None,
                scheduler_multiplier_reject_reason: None,
            })
        })
        .map_err(|error| format!("查询富路由候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析富路由候选失败: {error}"))?;

    let advanced_settings = parse_scheduler_advanced_settings(&read_setting_or_default(
        connection,
        "scheduler_advanced_settings_json",
        "",
    )?)?;
    let group_rate_interval_minutes: u16 =
        parse_setting_or_default(connection, "group_rate_interval_minutes", "20")?;
    let group_rate_interval_ms = i64::from(group_rate_interval_minutes) * 60 * 1000;
    let now_ms = now_millis_for_services() as i64;

    let mut enriched_rows = Vec::with_capacity(rows.len());
    for mut row in rows {
        if row.candidate.api_key.trim().is_empty() {
            let Some(data_key) = data_key else {
                return Err("Station Key 已迁移为加密凭据，当前调用缺少解密密钥".to_string());
            };
            row.candidate.api_key =
                resolve_station_key_api_key(connection, data_key, &row.candidate.station_key_id)?;
        }
        row.economics = route_candidate_economics_from_connection(
            connection,
            &row.candidate.station_key_id,
            &row.candidate.station_id,
            None,
        )?;
        let multiplier_facts = load_multiplier_source_facts_from_connection(
            connection,
            &row.candidate.station_key_id,
        )?;
        row.scheduler_group_binding_id = multiplier_facts.group_binding_id.clone();
        row.scheduler_group_id_hash = multiplier_facts.group_id_hash.clone();
        row.scheduler_group_type = pricing_group_type_from_binding_metadata(
            multiplier_facts.group_category_override.as_deref(),
            multiplier_facts.inferred_group_category.as_deref(),
            multiplier_facts.group_name.as_deref(),
        );
        match resolve_effective_multiplier(
            multiplier_facts,
            now_ms,
            advanced_settings.multiplier_min_confidence,
            group_rate_interval_ms,
        ) {
            Ok(fact) => {
                row.scheduler_effective_multiplier = Some(fact);
                row.scheduler_multiplier_reject_reason = None;
            }
            Err(reason) => {
                row.scheduler_effective_multiplier = None;
                row.scheduler_multiplier_reject_reason = Some(reason);
            }
        }
        enriched_rows.push(row);
    }

    Ok(enriched_rows)
}

fn local_routing_read_candidates_from_connection(
    connection: &Connection,
) -> Result<Vec<LocalRoutingReadCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                k.id,
                k.station_id,
                s.name,
                k.name,
                COALESCE(c.supports_chat_completions, 1),
                COALESCE(c.supports_responses, 1),
                COALESCE(c.supports_embeddings, 0),
                COALESCE(c.supports_stream, 1),
                COALESCE(c.supports_tools, 0),
                COALESCE(c.supports_vision, 0),
                COALESCE(c.supports_reasoning, 0),
                COALESCE(c.model_allowlist_json, '[]'),
                COALESCE(c.model_blocklist_json, '[]'),
                COALESCE(c.preferred_models_json, '[]'),
                COALESCE(c.only_use_as_backup, 0),
                COALESCE(c.routing_tags_json, '[]'),
                COALESCE(c.updated_at, '0'),
                COALESCE(k.schedulable, 1),
                h.station_key_id,
                h.last_success_at,
                h.last_failure_at,
                h.consecutive_failures,
                h.success_count,
                h.failure_count,
                h.avg_latency_ms,
                h.last_error_summary,
                h.cooldown_until,
                h.updated_at
             FROM station_keys k
             JOIN stations s ON s.id = k.station_id
             LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
             LEFT JOIN station_key_health h ON h.station_key_id = k.id
             WHERE k.enabled = 1
               AND s.enabled = 1
               AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
             ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                      k.priority ASC,
                      k.created_at ASC,
                      k.id ASC",
        )
        .map_err(|error| format!("读取本地路由读模型候选失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let station_key_id = row.get::<_, String>(0)?;
            let station_id = row.get::<_, String>(1)?;
            let health_station_key_id = row.get::<_, Option<String>>(18)?;
            Ok(LocalRoutingReadCandidate {
                station_key_id: station_key_id.clone(),
                station_id,
                station_name: row.get(2)?,
                key_name: row.get(3)?,
                schedulable: i64_to_bool(row.get(17)?),
                capabilities: StationKeyCapabilities {
                    station_key_id,
                    supports_chat_completions: i64_to_bool(row.get(4)?),
                    supports_responses: i64_to_bool(row.get(5)?),
                    supports_embeddings: i64_to_bool(row.get(6)?),
                    supports_stream: i64_to_bool(row.get(7)?),
                    supports_tools: i64_to_bool(row.get(8)?),
                    supports_vision: i64_to_bool(row.get(9)?),
                    supports_reasoning: i64_to_bool(row.get(10)?),
                    model_allowlist: parse_json_string_list(row.get::<_, String>(11)?.as_str()),
                    model_blocklist: parse_json_string_list(row.get::<_, String>(12)?.as_str()),
                    preferred_models: parse_json_string_list(row.get::<_, String>(13)?.as_str()),
                    only_use_as_backup: i64_to_bool(row.get(14)?),
                    routing_tags: parse_json_string_list(row.get::<_, String>(15)?.as_str()),
                    updated_at: row.get(16)?,
                },
                health: health_station_key_id.map(|station_key_id| StationKeyHealth {
                    station_key_id,
                    last_success_at: row.get(19).ok().flatten(),
                    last_failure_at: row.get(20).ok().flatten(),
                    consecutive_failures: row.get(21).unwrap_or(0),
                    success_count: row.get(22).unwrap_or(0),
                    failure_count: row.get(23).unwrap_or(0),
                    avg_latency_ms: row.get(24).ok().flatten(),
                    last_error_summary: row.get(25).ok().flatten(),
                    cooldown_until: row.get(26).ok().flatten(),
                    updated_at: row.get(27).unwrap_or_else(|_| "0".to_string()),
                }),
                economics: None,
                scheduler_group_binding_id: None,
                scheduler_group_id_hash: None,
                scheduler_group_type: None,
                scheduler_effective_multiplier: None,
                scheduler_multiplier_reject_reason: None,
            })
        })
        .map_err(|error| format!("查询本地路由读模型候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析本地路由读模型候选失败: {error}"))?;

    let advanced_settings = parse_scheduler_advanced_settings(&read_setting_or_default(
        connection,
        "scheduler_advanced_settings_json",
        "",
    )?)?;
    let group_rate_interval_minutes: u16 =
        parse_setting_or_default(connection, "group_rate_interval_minutes", "20")?;
    let group_rate_interval_ms = i64::from(group_rate_interval_minutes) * 60 * 1000;
    let now_ms = now_millis_for_services() as i64;

    let mut enriched_rows = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.economics = route_candidate_economics_from_connection(
            connection,
            &row.station_key_id,
            &row.station_id,
            None,
        )?;
        let multiplier_facts =
            load_multiplier_source_facts_from_connection(connection, &row.station_key_id)?;
        row.scheduler_group_binding_id = multiplier_facts.group_binding_id.clone();
        row.scheduler_group_id_hash = multiplier_facts.group_id_hash.clone();
        row.scheduler_group_type = pricing_group_type_from_binding_metadata(
            multiplier_facts.group_category_override.as_deref(),
            multiplier_facts.inferred_group_category.as_deref(),
            multiplier_facts.group_name.as_deref(),
        );
        match resolve_effective_multiplier(
            multiplier_facts,
            now_ms,
            advanced_settings.multiplier_min_confidence,
            group_rate_interval_ms,
        ) {
            Ok(fact) => {
                row.scheduler_effective_multiplier = Some(fact);
                row.scheduler_multiplier_reject_reason = None;
            }
            Err(reason) => {
                row.scheduler_effective_multiplier = None;
                row.scheduler_multiplier_reject_reason = Some(reason);
            }
        }
        enriched_rows.push(row);
    }

    Ok(enriched_rows)
}

fn enabled_model_alias_pairs_from_connection(
    connection: &Connection,
) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT client_model, upstream_model
               FROM model_aliases
              WHERE enabled = 1
              ORDER BY created_at ASC",
        )
        .map_err(|error| format!("读取启用模型映射失败: {error}"))?;

    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| format!("查询启用模型映射失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析启用模型映射失败: {error}"))?;

    Ok(rows)
}

fn route_candidate_economics_from_connection(
    connection: &Connection,
    station_key_id: &str,
    station_id: &str,
    requested_model: Option<&str>,
) -> Result<Option<RouteCandidateEconomics>, String> {
    let pricing_rule = connection
        .query_row(
            "SELECT id, model, input_price, output_price, fixed_price, currency, source,
                    group_binding_id, rate_multiplier, normalization_status, confidence
               FROM pricing_rules
              WHERE station_id = ?1
                AND enabled = 1
                AND (station_key_id = ?2 OR station_key_id IS NULL)
                AND (
                    ?3 IS NULL
                    OR lower(model) = lower(?3)
                    OR (
                        normalization_status = 'group_rate_only'
                        AND input_price IS NULL
                        AND output_price IS NULL
                        AND fixed_price IS NULL
                    )
                )
              ORDER BY
                CASE
                    WHEN ?3 IS NOT NULL AND lower(model) = lower(?3) THEN 0
                    ELSE 1
                END,
                CASE WHEN station_key_id = ?2 THEN 0 ELSE 1 END,
                CASE WHEN normalization_status = 'complete' THEN 0 ELSE 1 END,
                CASE
                    WHEN input_price IS NOT NULL OR output_price IS NOT NULL OR fixed_price IS NOT NULL THEN 0
                    ELSE 1
                END,
                updated_at DESC,
                created_at DESC
              LIMIT 1",
            params![station_id, station_key_id, requested_model],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<f64>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, f64>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取价格规则失败: {error}"))?;

    let balance_snapshot = connection
        .query_row(
            "SELECT value, currency, low_balance_threshold, status, scope, collected_at
               FROM balance_snapshots
              WHERE station_id = ?1
                AND (station_key_id = ?2 OR (station_key_id IS NULL AND scope = 'station'))
              ORDER BY updated_at DESC, created_at DESC
              LIMIT 1",
            params![station_id, station_key_id],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取余额快照失败: {error}"))?;

    let station_key_group_rate = connection
        .query_row(
            "SELECT COALESCE(k.group_binding_id, b.id),
                    COALESCE(k.rate_multiplier, b.effective_rate_multiplier),
                    COALESCE(b.confidence, 0.8)
               FROM station_keys k
               LEFT JOIN station_group_bindings b ON b.id = k.group_binding_id
              WHERE k.id = ?1",
            params![station_key_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 分组费率失败: {error}"))?
        .and_then(|(group_binding_id, rate_multiplier, confidence)| {
            let multiplier = rate_multiplier?;
            if !multiplier.is_finite() || multiplier <= 0.0 {
                return None;
            }
            Some((group_binding_id, multiplier, confidence))
        });

    let base_price_economics = pricing_rule.as_ref().and_then(
        |(
            _id,
            rule_model,
            input_price,
            output_price,
            fixed_price,
            _currency,
            _source,
            group_binding_id,
            rate_multiplier,
            _normalization_status,
            confidence,
        )| {
            if input_price.is_some() || output_price.is_some() || fixed_price.is_some() {
                return None;
            }
            let multiplier = (*rate_multiplier)?;
            let model = requested_model
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(rule_model.as_str());
            let base_price = model_base_price_for_model(connection, model)
                .ok()
                .flatten()?;
            let (
                base_model,
                base_input_price,
                base_output_price,
                base_currency,
                base_source_label,
                base_confidence,
            ) = base_price;
            let (
                balance_value,
                balance_currency,
                low_balance_threshold,
                balance_status,
                balance_scope,
                balance_collected_at,
            ) = balance_snapshot.clone().unwrap_or((
                None,
                "unknown".to_string(),
                None,
                "unknown".to_string(),
                "unknown".to_string(),
                None,
            ));
            Some(RouteCandidateEconomics {
                pricing_rule_id: None,
                pricing_model: Some(base_model),
                group_binding_id: group_binding_id.clone(),
                rate_multiplier: Some(multiplier),
                normalization_status: Some("base_price_with_group_rate".to_string()),
                price_confidence: Some((*confidence).min(base_confidence)),
                base_input_price,
                base_output_price,
                base_fixed_price: None,
                estimated_input_price: base_input_price.map(|price| price * multiplier),
                estimated_output_price: base_output_price.map(|price| price * multiplier),
                fixed_price: None,
                price_currency: Some(base_currency),
                pricing_source: Some("model_base_price".to_string()),
                balance_status: Some(balance_status),
                balance_value,
                low_balance_threshold,
                balance_currency: Some(balance_currency),
                balance_scope: Some(balance_scope),
                balance_collected_at,
                economic_freshness: Some(base_source_label),
            })
        },
    );

    let direct_base_price_economics = requested_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|model| {
            let base_price = model_base_price_for_model(connection, model)
                .ok()
                .flatten()?;
            let (
                base_model,
                base_input_price,
                base_output_price,
                base_currency,
                base_source_label,
                base_confidence,
            ) = base_price;
            let (
                balance_value,
                balance_currency,
                low_balance_threshold,
                balance_status,
                balance_scope,
                balance_collected_at,
            ) = balance_snapshot.clone().unwrap_or((
                None,
                "unknown".to_string(),
                None,
                "unknown".to_string(),
                "unknown".to_string(),
                None,
            ));
            Some(RouteCandidateEconomics {
                pricing_rule_id: None,
                pricing_model: Some(base_model),
                group_binding_id: None,
                rate_multiplier: Some(1.0),
                normalization_status: Some("base_price_only".to_string()),
                price_confidence: Some(base_confidence),
                base_input_price,
                base_output_price,
                base_fixed_price: None,
                estimated_input_price: base_input_price,
                estimated_output_price: base_output_price,
                fixed_price: None,
                price_currency: Some(base_currency),
                pricing_source: Some("model_base_price".to_string()),
                balance_status: Some(balance_status),
                balance_value,
                low_balance_threshold,
                balance_currency: Some(balance_currency),
                balance_scope: Some(balance_scope),
                balance_collected_at,
                economic_freshness: Some(base_source_label),
            })
        });

    let station_key_base_price_economics =
        station_key_group_rate.and_then(|(group_binding_id, multiplier, confidence)| {
            let model = requested_model
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let base_price = model_base_price_for_model(connection, model)
                .ok()
                .flatten()?;
            let (
                base_model,
                base_input_price,
                base_output_price,
                base_currency,
                base_source_label,
                base_confidence,
            ) = base_price;
            let (
                balance_value,
                balance_currency,
                low_balance_threshold,
                balance_status,
                balance_scope,
                balance_collected_at,
            ) = balance_snapshot.clone().unwrap_or((
                None,
                "unknown".to_string(),
                None,
                "unknown".to_string(),
                "unknown".to_string(),
                None,
            ));
            Some(RouteCandidateEconomics {
                pricing_rule_id: None,
                pricing_model: Some(base_model),
                group_binding_id,
                rate_multiplier: Some(multiplier),
                normalization_status: Some("base_price_with_group_rate".to_string()),
                price_confidence: Some(confidence.min(base_confidence)),
                base_input_price,
                base_output_price,
                base_fixed_price: None,
                estimated_input_price: base_input_price.map(|price| price * multiplier),
                estimated_output_price: base_output_price.map(|price| price * multiplier),
                fixed_price: None,
                price_currency: Some(base_currency),
                pricing_source: Some("model_base_price".to_string()),
                balance_status: Some(balance_status),
                balance_value,
                low_balance_threshold,
                balance_currency: Some(balance_currency),
                balance_scope: Some(balance_scope),
                balance_collected_at,
                economic_freshness: Some(base_source_label),
            })
        });

    let economics = base_price_economics
        .or_else(|| {
            pricing_rule.map(
                |(
                    id,
                    model,
                    input_price,
                    output_price,
                    fixed_price,
                    currency,
                    source,
                    group_binding_id,
                    rate_multiplier,
                    normalization_status,
                    confidence,
                )| {
                    let (
                        balance_value,
                        balance_currency,
                        low_balance_threshold,
                        balance_status,
                        balance_scope,
                        balance_collected_at,
                    ) = balance_snapshot.clone().unwrap_or((
                        None,
                        "unknown".to_string(),
                        None,
                        "unknown".to_string(),
                        "unknown".to_string(),
                        None,
                    ));
                    RouteCandidateEconomics {
                        pricing_rule_id: Some(id),
                        pricing_model: Some(model),
                        group_binding_id,
                        rate_multiplier,
                        normalization_status: Some(normalization_status),
                        price_confidence: Some(confidence),
                        base_input_price: input_price,
                        base_output_price: output_price,
                        base_fixed_price: fixed_price,
                        estimated_input_price: input_price,
                        estimated_output_price: output_price,
                        fixed_price,
                        price_currency: Some(currency),
                        pricing_source: Some(source),
                        balance_status: Some(balance_status),
                        balance_value,
                        low_balance_threshold,
                        balance_currency: Some(balance_currency),
                        balance_scope: Some(balance_scope),
                        balance_collected_at,
                        economic_freshness: Some("latest_available".to_string()),
                    }
                },
            )
        })
        .or(station_key_base_price_economics)
        .or(direct_base_price_economics)
        .or_else(|| {
            balance_snapshot.map(
                |(
                    balance_value,
                    balance_currency,
                    low_balance_threshold,
                    balance_status,
                    balance_scope,
                    balance_collected_at,
                )| {
                    RouteCandidateEconomics {
                        pricing_rule_id: None,
                        pricing_model: None,
                        group_binding_id: None,
                        rate_multiplier: None,
                        normalization_status: None,
                        price_confidence: None,
                        base_input_price: None,
                        base_output_price: None,
                        base_fixed_price: None,
                        estimated_input_price: None,
                        estimated_output_price: None,
                        fixed_price: None,
                        price_currency: None,
                        pricing_source: None,
                        balance_status: Some(balance_status),
                        balance_value,
                        low_balance_threshold,
                        balance_currency: Some(balance_currency),
                        balance_scope: Some(balance_scope),
                        balance_collected_at,
                        economic_freshness: Some("balance_only".to_string()),
                    }
                },
            )
        });

    Ok(economics)
}

fn model_base_price_for_model(
    connection: &Connection,
    model: &str,
) -> Result<Option<(String, Option<f64>, Option<f64>, String, String, f64)>, String> {
    connection
        .query_row(
            "SELECT model, input_price, output_price, currency, source_label,
                    CASE WHEN built_in = 1 THEN 0.95 ELSE 0.85 END
               FROM model_base_prices
              WHERE enabled = 1
                AND lower(model) = lower(?1)
              ORDER BY built_in DESC, updated_at DESC, created_at DESC
              LIMIT 1",
            params![model.trim()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取模型基准价格失败: {error}"))
}

fn route_candidate_economics_by_station_key(
    connection: &Connection,
    station_key_id: &str,
    requested_model: Option<&str>,
) -> Result<Option<RouteCandidateEconomics>, String> {
    let station_id: Option<String> = connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?;
    let Some(station_id) = station_id else {
        return Ok(None);
    };
    route_candidate_economics_from_connection(
        connection,
        station_key_id,
        &station_id,
        requested_model,
    )
}

fn list_pricing_rules_from_connection(connection: &Connection) -> Result<Vec<PricingRule>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, station_key_id, group_binding_id, group_name, tier_label,
                    model, input_price, output_price, fixed_price, rate_multiplier, currency,
                    unit, price_type, base_price_source, normalization_status, source, confidence,
                    enabled, note, collected_at, valid_from, valid_until, created_at, updated_at
               FROM pricing_rules
              ORDER BY updated_at DESC, created_at DESC",
        )
        .map_err(|error| format!("读取价格规则失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_pricing_rule)
        .map_err(|error| format!("查询价格规则失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析价格规则失败: {error}"))?;
    Ok(rows)
}

fn upsert_pricing_rule_in_connection(
    connection: &Connection,
    input: UpsertPricingRuleInput,
) -> Result<PricingRule, String> {
    let input = sanitize_pricing_rule_input(input);
    validate_station_exists(connection, &input.station_id)?;
    if let Some(station_key_id) = input.station_key_id.as_deref() {
        validate_station_key_exists(connection, station_key_id)?;
    }
    if input.model.trim().is_empty() {
        return Err("模型不能为空".to_string());
    }
    let previous_rule = connection
        .query_row(
            "SELECT id, station_id, station_key_id, group_binding_id, group_name, tier_label,
                    model, input_price, output_price, fixed_price, rate_multiplier, currency,
                    unit, price_type, base_price_source, normalization_status, source, confidence,
                    enabled, note, collected_at, valid_from, valid_until, created_at, updated_at
               FROM pricing_rules
              WHERE station_id = ?1
                AND COALESCE(station_key_id, '') = COALESCE(?2, '')
                AND COALESCE(group_binding_id, '') = COALESCE(?3, '')
                AND COALESCE(group_name, '') = COALESCE(?4, '')
                AND model = ?5
              ORDER BY updated_at DESC
              LIMIT 1",
            params![
                &input.station_id,
                normalize_optional_string(input.station_key_id.clone()),
                normalize_optional_string(input.group_binding_id.clone()),
                normalize_optional_string(input.group_name.clone()),
                input.model.trim(),
            ],
            row_to_pricing_rule,
        )
        .optional()
        .map_err(|error| format!("读取旧价格失败: {error}"))?;
    let confidence = clamp_confidence(input.confidence);
    let id = input.id.unwrap_or_else(|| generate_id("pricing"));
    let now = now_string();
    connection
        .execute(
            "INSERT INTO pricing_rules (
                id, station_id, station_key_id, group_binding_id, group_name, tier_label, model,
                input_price, output_price, fixed_price, rate_multiplier, currency, unit,
                price_type, base_price_source, normalization_status, source, confidence, enabled,
                note, collected_at, valid_from, valid_until, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
             ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                group_binding_id = excluded.group_binding_id,
                group_name = excluded.group_name,
                tier_label = excluded.tier_label,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                fixed_price = excluded.fixed_price,
                rate_multiplier = excluded.rate_multiplier,
                currency = excluded.currency,
                unit = excluded.unit,
                price_type = excluded.price_type,
                base_price_source = excluded.base_price_source,
                normalization_status = excluded.normalization_status,
                source = excluded.source,
                confidence = excluded.confidence,
                enabled = excluded.enabled,
                note = excluded.note,
                collected_at = excluded.collected_at,
                valid_from = excluded.valid_from,
                valid_until = excluded.valid_until,
                updated_at = excluded.updated_at",
            params![
                id,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                normalize_optional_string(input.group_binding_id),
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
                input.model.trim(),
                input.input_price,
                input.output_price,
                input.fixed_price,
                input.rate_multiplier,
                normalize_currency(input.currency),
                normalize_unit(input.unit),
                input.price_type.trim(),
                normalize_optional_string(input.base_price_source),
                normalize_optional_string(input.normalization_status)
                    .unwrap_or_else(|| "manual".to_string()),
                input.source.trim(),
                confidence,
                bool_to_i64(input.enabled),
                normalize_optional_string(input.note),
                normalize_optional_string(input.collected_at),
                normalize_optional_string(input.valid_from),
                normalize_optional_string(input.valid_until),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存价格规则失败: {error}"))?;
    let saved = pricing_rule_by_id(connection, &id)?;
    if saved
        .valid_until
        .as_deref()
        .map(timestamp_is_past)
        .unwrap_or(false)
    {
        let event = crate::services::change_events::price_expired_event(
            &saved.station_id,
            &saved.id,
            &saved.model,
            saved.valid_until.as_deref(),
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
    if let Some(previous) = previous_rule {
        if let Some(event) = crate::services::change_events::price_changed_event(
            &saved.station_id,
            &saved.id,
            &saved.model,
            saved.group_name.as_deref(),
            previous.output_price,
            saved.output_price,
            &saved.currency,
        ) {
            let _ = upsert_change_event_in_connection(connection, event);
        }
    }
    Ok(saved)
}

fn list_model_base_prices_from_connection(
    connection: &Connection,
) -> Result<Vec<ModelBasePrice>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, provider, model, input_price, output_price, currency, unit,
                    source_url, source_label, source_checked_at, enabled, built_in, note,
                    created_at, updated_at
               FROM model_base_prices
              ORDER BY enabled DESC, provider ASC, model ASC, updated_at DESC",
        )
        .map_err(|error| format!("读取模型基准价格失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_model_base_price)
        .map_err(|error| format!("查询模型基准价格失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析模型基准价格失败: {error}"))?;
    Ok(rows)
}

fn upsert_model_base_price_in_connection(
    connection: &Connection,
    input: UpsertModelBasePriceInput,
) -> Result<ModelBasePrice, String> {
    let provider = input.provider.trim();
    if provider.is_empty() {
        return Err("模型服务商不能为空".to_string());
    }
    let model = input.model.trim();
    if model.is_empty() {
        return Err("模型名称不能为空".to_string());
    }
    let source_label = input.source_label.trim();
    if source_label.is_empty() {
        return Err("价格来源名称不能为空".to_string());
    }
    let id = input.id.unwrap_or_else(|| generate_id("model_base_price"));
    let now = now_string();
    connection
        .execute(
            "INSERT INTO model_base_prices (
                id, provider, model, input_price, output_price, currency, unit,
                source_url, source_label, source_checked_at, enabled, built_in, note,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                currency = excluded.currency,
                unit = excluded.unit,
                source_url = excluded.source_url,
                source_label = excluded.source_label,
                source_checked_at = excluded.source_checked_at,
                enabled = excluded.enabled,
                built_in = excluded.built_in,
                note = excluded.note,
                updated_at = excluded.updated_at",
            params![
                id,
                provider,
                model,
                input.input_price,
                input.output_price,
                normalize_currency(input.currency),
                normalize_unit(input.unit),
                input.source_url.trim(),
                source_label,
                normalize_optional_string(input.source_checked_at),
                bool_to_i64(input.enabled),
                bool_to_i64(input.built_in),
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存模型基准价格失败: {error}"))?;
    model_base_price_by_id(connection, &id)
}

fn delete_pricing_rule_from_connection(connection: &Connection, id: &str) -> Result<(), String> {
    let deleted = connection
        .execute("DELETE FROM pricing_rules WHERE id = ?1", params![id])
        .map_err(|error| format!("删除价格规则失败: {error}"))?;
    if deleted == 0 {
        return Err("价格规则不存在，无法删除".to_string());
    }
    Ok(())
}

fn list_balance_snapshots_from_connection(
    connection: &Connection,
) -> Result<Vec<BalanceSnapshot>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
                    used_value, total_value, today_request_count, total_request_count,
                    today_consumption, total_consumption, today_base_consumption, total_base_consumption,
                    today_token_count, total_token_count,
                    today_input_token_count, today_output_token_count, total_input_token_count,
                    total_output_token_count, account_concurrency_limit, low_balance_threshold, status, source, confidence,
                    collected_at, created_at, updated_at
               FROM balance_snapshots
              ORDER BY updated_at DESC, created_at DESC",
        )
        .map_err(|error| format!("读取余额快照失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_balance_snapshot)
        .map_err(|error| format!("查询余额快照失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析余额快照失败: {error}"))?;
    Ok(rows)
}

fn list_current_station_balance_snapshots_from_connection(
    connection: &Connection,
) -> Result<Vec<BalanceSnapshot>, String> {
    let mut statement = connection
        .prepare(
            "SELECT b.id, b.station_id, b.station_key_id, b.scope, b.value, b.currency, b.credit_unit,
                    b.used_value, b.total_value, b.today_request_count, b.total_request_count,
                    b.today_consumption, b.total_consumption, b.today_base_consumption, b.total_base_consumption,
                    b.today_token_count, b.total_token_count,
                    b.today_input_token_count, b.today_output_token_count, b.total_input_token_count,
                    b.total_output_token_count, b.account_concurrency_limit, b.low_balance_threshold, b.status, b.source, b.confidence,
                    b.collected_at, b.created_at, b.updated_at
               FROM balance_snapshots b
              WHERE b.id IN (
                    SELECT (
                        SELECT latest.id
                          FROM balance_snapshots latest INDEXED BY idx_balance_snapshots_station_scope_updated
                         WHERE latest.station_id = s.id
                           AND latest.scope = 'station'
                         ORDER BY latest.updated_at DESC, latest.created_at DESC, latest.id DESC
                         LIMIT 1
                    )
                      FROM stations s
              )
              ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC",
        )
        .map_err(|error| format!("Failed to read current station balances: {error}"))?;
    let rows = statement
        .query_map([], row_to_balance_snapshot)
        .map_err(|error| format!("Failed to query current station balances: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to parse current station balances: {error}"))?;
    Ok(rows)
}

fn list_balance_snapshots_for_station_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<BalanceSnapshot>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
                    used_value, total_value, today_request_count, total_request_count,
                    today_consumption, total_consumption, today_base_consumption, total_base_consumption,
                    today_token_count, total_token_count,
                    today_input_token_count, today_output_token_count, total_input_token_count,
                    total_output_token_count, account_concurrency_limit, low_balance_threshold, status, source, confidence,
                    collected_at, created_at, updated_at
               FROM balance_snapshots
              WHERE station_id = ?1
              ORDER BY updated_at DESC, created_at DESC",
        )
        .map_err(|error| format!("读取中转站余额快照失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_balance_snapshot)
        .map_err(|error| format!("查询中转站余额快照失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析中转站余额快照失败: {error}"))?;
    Ok(rows)
}

pub(crate) fn upsert_balance_snapshot_in_connection(
    connection: &Connection,
    input: UpsertBalanceSnapshotInput,
) -> Result<BalanceSnapshot, String> {
    validate_station_exists(connection, &input.station_id)?;
    if let Some(station_key_id) = input.station_key_id.as_deref() {
        validate_station_key_exists(connection, station_key_id)?;
    }
    if input.scope.trim().is_empty() {
        return Err("余额作用域不能为空".to_string());
    }
    if input.status.trim().is_empty() {
        return Err("余额状态不能为空".to_string());
    }
    let confidence = clamp_confidence(input.confidence);
    let id = input.id.unwrap_or_else(|| generate_id("balance"));
    let now = now_string();
    connection
        .execute(
            "INSERT INTO balance_snapshots (
                id, station_id, station_key_id, scope, value, currency, credit_unit,
                used_value, total_value, today_request_count, total_request_count,
                today_consumption, total_consumption, today_base_consumption, total_base_consumption,
                today_token_count, total_token_count,
                today_input_token_count, today_output_token_count, total_input_token_count,
                total_output_token_count, account_concurrency_limit, low_balance_threshold, status, source, confidence,
                collected_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)
             ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                scope = excluded.scope,
                value = excluded.value,
                currency = excluded.currency,
                credit_unit = excluded.credit_unit,
                used_value = excluded.used_value,
                total_value = excluded.total_value,
                today_request_count = excluded.today_request_count,
                total_request_count = excluded.total_request_count,
                today_consumption = excluded.today_consumption,
                total_consumption = excluded.total_consumption,
                today_base_consumption = excluded.today_base_consumption,
                total_base_consumption = excluded.total_base_consumption,
                today_token_count = excluded.today_token_count,
                total_token_count = excluded.total_token_count,
                today_input_token_count = excluded.today_input_token_count,
                today_output_token_count = excluded.today_output_token_count,
                total_input_token_count = excluded.total_input_token_count,
                total_output_token_count = excluded.total_output_token_count,
                account_concurrency_limit = excluded.account_concurrency_limit,
                low_balance_threshold = excluded.low_balance_threshold,
                status = excluded.status,
                source = excluded.source,
                confidence = excluded.confidence,
                collected_at = excluded.collected_at,
                updated_at = excluded.updated_at",
            params![
                id,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                normalize_scope(input.scope)?,
                input.value,
                normalize_currency(input.currency),
                normalize_optional_string(input.credit_unit),
                input.used_value,
                input.total_value,
                input.today_request_count,
                input.total_request_count,
                input.today_consumption,
                input.total_consumption,
                input.today_base_consumption,
                input.total_base_consumption,
                input.today_token_count,
                input.total_token_count,
                input.today_input_token_count,
                input.today_output_token_count,
                input.total_input_token_count,
                input.total_output_token_count,
                input.account_concurrency_limit,
                input.low_balance_threshold,
                normalize_balance_status(input.status)?,
                input.source.trim(),
                confidence,
                normalize_optional_string(input.collected_at),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存余额快照失败: {error}"))?;
    let saved = balance_snapshot_by_id(connection, &id)?;
    if saved.scope == "station" {
        if let Some(event) = crate::services::change_events::station_balance_event(
            &saved.station_id,
            &saved.status,
            saved.value,
            saved.low_balance_threshold,
        ) {
            let _ = upsert_change_event_in_connection(connection, event);
        }
    }
    Ok(saved)
}

fn row_to_pricing_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<PricingRule> {
    Ok(PricingRule {
        id: row.get(0)?,
        station_id: row.get(1)?,
        station_key_id: row.get(2)?,
        group_binding_id: row.get(3)?,
        group_name: row.get(4)?,
        tier_label: row.get(5)?,
        model: row.get(6)?,
        input_price: row.get(7)?,
        output_price: row.get(8)?,
        fixed_price: row.get(9)?,
        rate_multiplier: row.get(10)?,
        currency: row.get(11)?,
        unit: row.get(12)?,
        price_type: row.get(13)?,
        base_price_source: row.get(14)?,
        normalization_status: row.get(15)?,
        source: row.get(16)?,
        confidence: row.get(17)?,
        enabled: i64_to_bool(row.get(18)?),
        note: row.get(19)?,
        collected_at: row.get(20)?,
        valid_from: row.get(21)?,
        valid_until: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
    })
}

fn row_to_model_base_price(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelBasePrice> {
    Ok(ModelBasePrice {
        id: row.get(0)?,
        provider: row.get(1)?,
        model: row.get(2)?,
        input_price: row.get(3)?,
        output_price: row.get(4)?,
        currency: row.get(5)?,
        unit: row.get(6)?,
        source_url: row.get(7)?,
        source_label: row.get(8)?,
        source_checked_at: row.get(9)?,
        enabled: i64_to_bool(row.get(10)?),
        built_in: i64_to_bool(row.get(11)?),
        note: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn row_to_balance_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<BalanceSnapshot> {
    Ok(BalanceSnapshot {
        id: row.get(0)?,
        station_id: row.get(1)?,
        station_key_id: row.get(2)?,
        scope: row.get(3)?,
        value: row.get(4)?,
        currency: row.get(5)?,
        credit_unit: row.get(6)?,
        used_value: row.get(7)?,
        total_value: row.get(8)?,
        today_request_count: row.get(9)?,
        total_request_count: row.get(10)?,
        today_consumption: row.get(11)?,
        total_consumption: row.get(12)?,
        today_base_consumption: row.get(13)?,
        total_base_consumption: row.get(14)?,
        today_token_count: row.get(15)?,
        total_token_count: row.get(16)?,
        today_input_token_count: row.get(17)?,
        today_output_token_count: row.get(18)?,
        total_input_token_count: row.get(19)?,
        total_output_token_count: row.get(20)?,
        account_concurrency_limit: row.get(21)?,
        low_balance_threshold: row.get(22)?,
        status: row.get(23)?,
        source: row.get(24)?,
        confidence: row.get(25)?,
        collected_at: row.get(26)?,
        created_at: row.get(27)?,
        updated_at: row.get(28)?,
    })
}

fn model_base_price_by_id(connection: &Connection, id: &str) -> Result<ModelBasePrice, String> {
    connection
        .query_row(
            "SELECT id, provider, model, input_price, output_price, currency, unit,
                    source_url, source_label, source_checked_at, enabled, built_in, note,
                    created_at, updated_at
               FROM model_base_prices
              WHERE id = ?1",
            params![id],
            row_to_model_base_price,
        )
        .optional()
        .map_err(|error| format!("读取模型基准价格失败: {error}"))?
        .ok_or_else(|| "模型基准价格不存在".to_string())
}

fn pricing_rule_by_id(connection: &Connection, id: &str) -> Result<PricingRule, String> {
    connection
        .query_row(
            "SELECT id, station_id, station_key_id, group_binding_id, group_name, tier_label,
                    model, input_price, output_price, fixed_price, rate_multiplier, currency,
                    unit, price_type, base_price_source, normalization_status, source, confidence,
                    enabled, note, collected_at, valid_from, valid_until, created_at, updated_at
               FROM pricing_rules
              WHERE id = ?1",
            params![id],
            row_to_pricing_rule,
        )
        .optional()
        .map_err(|error| format!("读取价格规则失败: {error}"))?
        .ok_or_else(|| "价格规则不存在".to_string())
}

fn balance_snapshot_by_id(connection: &Connection, id: &str) -> Result<BalanceSnapshot, String> {
    connection
        .query_row(
            "SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
                    used_value, total_value, today_request_count, total_request_count,
                    today_consumption, total_consumption, today_base_consumption, total_base_consumption,
                    today_token_count, total_token_count,
                    today_input_token_count, today_output_token_count, total_input_token_count,
                    total_output_token_count, account_concurrency_limit, low_balance_threshold, status, source, confidence,
                    collected_at, created_at, updated_at
               FROM balance_snapshots
              WHERE id = ?1",
            params![id],
            row_to_balance_snapshot,
        )
        .optional()
        .map_err(|error| format!("读取余额快照失败: {error}"))?
        .ok_or_else(|| "余额快照不存在".to_string())
}

fn list_change_events_from_connection(connection: &Connection) -> Result<Vec<ChangeEvent>, String> {
    let mut statement = connection
        .prepare(
            "SELECT change_events.id, change_events.severity, change_events.event_type, change_events.status,
                    change_events.title, change_events.message, change_events.object_type, change_events.object_id,
                    change_events.station_id, stations.name AS station_name,
                    change_events.station_key_id, change_events.pricing_rule_id, change_events.request_log_id,
                    change_events.old_value_json, change_events.new_value_json, change_events.impact_json,
                    change_events.dedupe_key, change_events.source,
                    change_events.detected_at, change_events.resolved_at, change_events.created_at, change_events.updated_at
               FROM change_events
               LEFT JOIN stations ON stations.id = change_events.station_id
              WHERE NOT (
                    change_events.event_type IN ('model_added', 'model_removed')
                    AND COALESCE(stations.station_type, '') = 'newapi'
                 )
              ORDER BY change_events.updated_at DESC, change_events.detected_at DESC",
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_change_event)
        .map_err(|error| format!("查询变更事件失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析变更事件失败: {error}"))?;
    Ok(rows)
}

fn list_change_events_for_station_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<ChangeEvent>, String> {
    let mut statement = connection
        .prepare(
            "SELECT change_events.id, change_events.severity, change_events.event_type, change_events.status,
                    change_events.title, change_events.message, change_events.object_type, change_events.object_id,
                    change_events.station_id, stations.name AS station_name,
                    change_events.station_key_id, change_events.pricing_rule_id, change_events.request_log_id,
                    change_events.old_value_json, change_events.new_value_json, change_events.impact_json,
                    change_events.dedupe_key, change_events.source,
                    change_events.detected_at, change_events.resolved_at, change_events.created_at, change_events.updated_at
               FROM change_events
               LEFT JOIN stations ON stations.id = change_events.station_id
              WHERE change_events.station_id = ?1
                AND NOT (
                    change_events.event_type IN ('model_added', 'model_removed')
                    AND COALESCE(stations.station_type, '') = 'newapi'
                 )
              ORDER BY change_events.updated_at DESC, change_events.detected_at DESC",
        )
        .map_err(|error| format!("读取中转站变更事件失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_change_event)
        .map_err(|error| format!("查询中转站变更事件失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析中转站变更事件失败: {error}"))?;
    Ok(rows)
}

fn upsert_change_event_in_connection(
    connection: &Connection,
    input: UpsertChangeEventInput,
) -> Result<ChangeEvent, String> {
    if input.severity.trim().is_empty() {
        return Err("变更级别不能为空".to_string());
    }
    if input.event_type.trim().is_empty() {
        return Err("变更类型不能为空".to_string());
    }
    if input.title.trim().is_empty() {
        return Err("变更标题不能为空".to_string());
    }
    if input.dedupe_key.trim().is_empty() {
        return Err("变更去重键不能为空".to_string());
    }
    let id = generate_id("change");
    let now = now_string();
    let dedupe_key = input.dedupe_key.trim().to_string();
    connection
        .execute(
            "INSERT INTO change_events (
                id, severity, event_type, status, title, message, object_type, object_id,
                station_id, station_key_id, pricing_rule_id, request_log_id,
                old_value_json, new_value_json, impact_json, dedupe_key, source,
                detected_at, resolved_at, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, NULL, ?19, ?20)
             ON CONFLICT(dedupe_key) DO UPDATE SET
                severity = excluded.severity,
                event_type = excluded.event_type,
                status = CASE
                    WHEN excluded.event_type = 'balance_depleted' THEN change_events.status
                    WHEN excluded.event_type = 'collector_failed'
                     AND change_events.status != 'resolved' THEN change_events.status
                    WHEN change_events.status = 'dismissed' THEN change_events.status
                    ELSE 'unread'
                END,
                title = excluded.title,
                message = excluded.message,
                object_type = excluded.object_type,
                object_id = excluded.object_id,
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                pricing_rule_id = excluded.pricing_rule_id,
                request_log_id = excluded.request_log_id,
                old_value_json = excluded.old_value_json,
                new_value_json = excluded.new_value_json,
                impact_json = excluded.impact_json,
                source = excluded.source,
                detected_at = CASE
                    WHEN excluded.event_type = 'balance_depleted' THEN change_events.detected_at
                    WHEN excluded.event_type = 'collector_failed'
                     AND change_events.status != 'resolved' THEN change_events.detected_at
                    ELSE excluded.detected_at
                END,
                resolved_at = CASE
                    WHEN excluded.event_type = 'balance_depleted' THEN change_events.resolved_at
                    WHEN excluded.event_type = 'collector_failed'
                     AND change_events.status != 'resolved' THEN change_events.resolved_at
                    ELSE NULL
                END,
                updated_at = CASE
                    WHEN excluded.event_type = 'balance_depleted' THEN change_events.updated_at
                    WHEN excluded.event_type = 'collector_failed'
                     AND change_events.status != 'resolved' THEN change_events.updated_at
                    ELSE excluded.updated_at
                END",
            params![
                id,
                input.severity.trim(),
                input.event_type.trim(),
                STATUS_UNREAD,
                input.title.trim(),
                input.message.trim(),
                input.object_type.trim(),
                normalize_optional_string(input.object_id),
                normalize_optional_string(input.station_id),
                normalize_optional_string(input.station_key_id),
                normalize_optional_string(input.pricing_rule_id),
                normalize_optional_string(input.request_log_id),
                normalize_optional_string(input.old_value_json),
                normalize_optional_string(input.new_value_json),
                normalize_optional_string(input.impact_json),
                dedupe_key,
                input.source.trim(),
                now,
                now,
                now,
            ],
        )
        .map_err(|error| format!("写入变更事件失败: {error}"))?;
    change_event_by_dedupe_key(connection, &dedupe_key)
}

fn change_event_by_dedupe_key(
    connection: &Connection,
    dedupe_key: &str,
) -> Result<ChangeEvent, String> {
    connection
        .query_row(
            "SELECT change_events.id, change_events.severity, change_events.event_type, change_events.status,
                    change_events.title, change_events.message, change_events.object_type, change_events.object_id,
                    change_events.station_id, stations.name AS station_name,
                    change_events.station_key_id, change_events.pricing_rule_id, change_events.request_log_id,
                    change_events.old_value_json, change_events.new_value_json, change_events.impact_json,
                    change_events.dedupe_key, change_events.source,
                    change_events.detected_at, change_events.resolved_at, change_events.created_at, change_events.updated_at
               FROM change_events
               LEFT JOIN stations ON stations.id = change_events.station_id
              WHERE change_events.dedupe_key = ?1",
            params![dedupe_key],
            row_to_change_event,
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))
}

fn change_event_by_id(connection: &Connection, id: &str) -> Result<ChangeEvent, String> {
    connection
        .query_row(
            "SELECT change_events.id, change_events.severity, change_events.event_type, change_events.status,
                    change_events.title, change_events.message, change_events.object_type, change_events.object_id,
                    change_events.station_id, stations.name AS station_name,
                    change_events.station_key_id, change_events.pricing_rule_id, change_events.request_log_id,
                    change_events.old_value_json, change_events.new_value_json, change_events.impact_json,
                    change_events.dedupe_key, change_events.source,
                    change_events.detected_at, change_events.resolved_at, change_events.created_at, change_events.updated_at
               FROM change_events
               LEFT JOIN stations ON stations.id = change_events.station_id
              WHERE change_events.id = ?1",
            params![id],
            row_to_change_event,
        )
        .optional()
        .map_err(|error| format!("读取变更事件失败: {error}"))?
        .ok_or_else(|| "变更事件不存在".to_string())
}

fn update_change_event_status_in_connection(
    connection: &Connection,
    id: &str,
    status: &str,
) -> Result<ChangeEvent, String> {
    let now = now_string();
    let updated = connection
        .execute(
            "UPDATE change_events SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, now],
        )
        .map_err(|error| format!("更新变更事件状态失败: {error}"))?;
    if updated == 0 {
        return Err("变更事件不存在".to_string());
    }
    change_event_by_id(connection, id)
}

fn resolve_change_event_in_connection(
    connection: &Connection,
    id: &str,
) -> Result<ChangeEvent, String> {
    let now = now_string();
    let updated = connection
        .execute(
            "UPDATE change_events SET status = ?2, resolved_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![id, STATUS_RESOLVED, now],
        )
        .map_err(|error| format!("解决变更事件失败: {error}"))?;
    if updated == 0 {
        return Err("变更事件不存在".to_string());
    }
    change_event_by_id(connection, id)
}

fn resolve_change_event_by_dedupe_key_in_connection(
    connection: &Connection,
    dedupe_key: &str,
) -> Result<(), String> {
    let now = now_string();
    connection
        .execute(
            "UPDATE change_events
                SET status = ?2, resolved_at = ?3, updated_at = ?3
              WHERE dedupe_key = ?1 AND status != ?2",
            params![dedupe_key, STATUS_RESOLVED, now],
        )
        .map_err(|error| format!("解决去重变更事件失败: {error}"))?;
    Ok(())
}

fn row_to_change_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeEvent> {
    Ok(ChangeEvent {
        id: row.get(0)?,
        severity: row.get(1)?,
        event_type: row.get(2)?,
        status: row.get(3)?,
        title: row.get(4)?,
        message: row.get(5)?,
        object_type: row.get(6)?,
        object_id: row.get(7)?,
        station_id: row.get(8)?,
        station_name: row.get(9)?,
        station_key_id: row.get(10)?,
        pricing_rule_id: row.get(11)?,
        request_log_id: row.get(12)?,
        old_value_json: row.get(13)?,
        new_value_json: row.get(14)?,
        impact_json: row.get(15)?,
        dedupe_key: row.get(16)?,
        source: row.get(17)?,
        detected_at: row.get(18)?,
        resolved_at: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

fn clamp_confidence(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn normalize_currency(value: String) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

fn normalize_unit(value: String) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

fn normalize_scope(value: String) -> Result<String, String> {
    match value.trim() {
        "station" => Ok("station".to_string()),
        "station_key" => Ok("station_key".to_string()),
        _ => Err("余额作用域无效".to_string()),
    }
}

fn normalize_balance_status(value: String) -> Result<String, String> {
    match value.trim() {
        "unknown" | "normal" | "low" | "depleted" => Ok(value.trim().to_string()),
        _ => Err("余额状态无效".to_string()),
    }
}

fn simulate_route_in_connection(
    connection: &Connection,
    data_dir: &str,
    input: RouteSimulationInput,
) -> Result<RouteSimulationResult, String> {
    let settings = settings_from_connection(connection, data_dir, None).ok();
    let policy = input.policy.unwrap_or_else(|| {
        settings
            .as_ref()
            .map(|settings| parse_routing_policy_value(&settings.default_routing_strategy))
            .unwrap_or(RoutingPolicy::PriorityFallback)
    });
    let allow_depleted_fallback = settings
        .as_ref()
        .map(|settings| settings.allow_depleted_fallback)
        .unwrap_or(false);
    let now_ms = now_millis_for_services() as i64;
    let routing_group_filter = input
        .routing_group_filter
        .clone()
        .or_else(|| {
            settings
                .as_ref()
                .map(|settings| settings.default_routing_group_filter.clone())
        })
        .unwrap_or_default();
    let max_rate_multiplier = input.max_rate_multiplier.or_else(|| {
        settings
            .as_ref()
            .and_then(|settings| settings.max_rate_multiplier)
    });
    let aliases = enabled_model_alias_pairs_from_connection(connection)?;
    if matches!(policy, RoutingPolicy::AutomaticBalanced) {
        let max_rate_multiplier = max_rate_multiplier
            .ok_or_else(|| "routing_multiplier_limit_not_configured".to_string())?;
        if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
            return Err("routing_multiplier_limit_not_configured".to_string());
        }
        let mapped_model =
            crate::services::proxy::routing_policy::mapped_model(input.model.as_deref(), &aliases);
        let scheduler_request = crate::services::proxy::scheduler::types::ScheduleRequest {
            endpoint: input.endpoint,
            requested_model: input.model.clone(),
            mapped_model: mapped_model.clone(),
            routing_group_filter: routing_group_filter.clone(),
            stream: input.stream,
            uses_tools: input.uses_tools,
            uses_vision: input.uses_vision,
            uses_reasoning: input.uses_reasoning,
            max_rate_multiplier,
            session_hash: input.session_hash.clone(),
            previous_response_id: input.previous_response_id.clone(),
            excluded_key_ids: Vec::new(),
            now_ms,
        };
        let scheduler_candidates =
            load_scheduler_candidates_from_connection(connection, &routing_group_filter, now_ms)?;
        let metrics = RuntimeMetricsRegistry::default();
        let capacity = CapacityRegistry::default();
        let mut affinity = AffinityStore::default();
        let advanced = settings
            .as_ref()
            .map(|settings| settings.scheduler_advanced_settings.clone())
            .unwrap_or_else(SchedulerAdvancedSettings::default);
        let local_candidates = local_routing_read_candidates_from_connection(connection)?;
        let decision = schedule_once(
            &scheduler_request,
            &scheduler_candidates,
            &metrics,
            &capacity,
            &mut affinity,
            &advanced,
        );
        let (selected_station_key_id, candidates, scheduler_error_code) = match decision {
            Ok(decision) => (
                decision.selected_station_key_id.clone(),
                automatic_simulation_explanations(
                    &routing_group_filter,
                    &scheduler_candidates,
                    &local_candidates,
                    &decision.candidate_decisions,
                    mapped_model.clone(),
                ),
                None,
            ),
            Err(error) => {
                let candidates = automatic_simulation_explanations(
                    &routing_group_filter,
                    &scheduler_candidates,
                    &local_candidates,
                    &error.candidate_decisions,
                    mapped_model.clone(),
                );
                return Ok(RouteSimulationResult {
                    selected_station_key_id: None,
                    selected_station_id: None,
                    mapped_model,
                    policy,
                    max_rate_multiplier: Some(max_rate_multiplier),
                    routing_group_filter,
                    scheduler_error_code: Some(error.code.to_string()),
                    candidates,
                    message: format!("Automatic scheduler rejected request: {}", error.code),
                });
            }
        };
        let selected_station_id = selected_station_key_id.as_ref().and_then(|station_key_id| {
            scheduler_candidates
                .iter()
                .find(|candidate| &candidate.station_key_id == station_key_id)
                .map(|candidate| candidate.station_id.clone())
        });
        let message = selected_station_key_id
            .as_ref()
            .map(|station_key_id| format!("Automatic scheduler selected {station_key_id}"))
            .unwrap_or_else(|| "Automatic scheduler found no eligible route".to_string());

        return Ok(RouteSimulationResult {
            selected_station_key_id,
            selected_station_id,
            mapped_model,
            policy,
            max_rate_multiplier: Some(max_rate_multiplier),
            routing_group_filter,
            scheduler_error_code,
            candidates,
            message,
        });
    }
    let request = RouteRequest {
        endpoint: input.endpoint,
        model: input.model,
        stream: input.stream,
        uses_tools: input.uses_tools,
        uses_vision: input.uses_vision,
        uses_reasoning: input.uses_reasoning,
        policy: policy.clone(),
        max_rate_multiplier,
        routing_group_filter: routing_group_filter.clone(),
        session_hash: input.session_hash,
        previous_response_id: input.previous_response_id,
        excluded_key_ids: Vec::new(),
        current_station_key_id: None,
        allow_depleted_fallback,
        now_ms,
    };
    let candidates = proxy_rich_route_candidates_from_connection(connection)?;
    let selection = select_route_candidates(&request, candidates, &aliases)?;
    let selected = selection.accepted.first();
    let selected_station_key_id =
        selected.map(|candidate| candidate.candidate.station_key_id.clone());
    let selected_station_id = selected.map(|candidate| candidate.candidate.station_id.clone());
    let message = if let Some(candidate) = selected {
        format!(
            "将选择 {}，因为该 Key 满足请求模型、协议和健康状态要求。",
            candidate.key_name
        )
    } else {
        format!(
            "没有可用 Station Key 支持该请求：endpoint={:?} model={} stream={}",
            request.endpoint,
            request.model.as_deref().unwrap_or("<none>"),
            request.stream
        )
    };

    Ok(RouteSimulationResult {
        selected_station_key_id,
        selected_station_id,
        mapped_model: selection.mapped_model,
        policy,
        max_rate_multiplier,
        routing_group_filter,
        scheduler_error_code: None,
        candidates: selection.explanations,
        message,
    })
}

fn parse_routing_policy_value(value: &str) -> RoutingPolicy {
    match value {
        "automatic_balanced" | "automatic" => RoutingPolicy::AutomaticBalanced,
        "stable_first" | "stable" => RoutingPolicy::StableFirst,
        "backup_only" => RoutingPolicy::BackupOnly,
        "cheap_first" => RoutingPolicy::CheapFirst,
        "cost_stable_first" => RoutingPolicy::CostStableFirst,
        _ => RoutingPolicy::PriorityFallback,
    }
}

fn automatic_simulation_explanations(
    routing_group_filter: &RoutingGroupFilter,
    scheduler_candidates: &[SchedulerCandidate],
    local_candidates: &[LocalRoutingReadCandidate],
    decisions: &[crate::services::proxy::scheduler::types::SchedulerCandidateDecision],
    mapped_model: Option<String>,
) -> Vec<crate::models::routing::RouteCandidateExplanation> {
    let local_by_key = local_candidates
        .iter()
        .map(|candidate| (candidate.station_key_id.as_str(), candidate))
        .collect::<std::collections::HashMap<_, _>>();
    let scheduler_by_key = scheduler_candidates
        .iter()
        .map(|candidate| (candidate.station_key_id.as_str(), candidate))
        .collect::<std::collections::HashMap<_, _>>();

    decisions
        .iter()
        .filter_map(|decision| {
            let scheduler_candidate = scheduler_by_key.get(decision.station_key_id.as_str())?;
            let local_candidate = local_by_key.get(decision.station_key_id.as_str());
            let economics = local_candidate.and_then(|candidate| candidate.economics.as_ref());
            Some(crate::models::routing::RouteCandidateExplanation {
                station_key_id: decision.station_key_id.clone(),
                station_id: decision.station_id.clone(),
                station_name: local_candidate
                    .map(|candidate| candidate.station_name.clone())
                    .unwrap_or_else(|| decision.station_id.clone()),
                key_name: local_candidate
                    .map(|candidate| candidate.key_name.clone())
                    .unwrap_or_else(|| decision.station_key_id.clone()),
                accepted: decision.accepted,
                score: decision
                    .score
                    .map(|score| (score * 1000.0).round() as i64)
                    .unwrap_or(i64::MAX),
                reasons: crate::services::proxy::scheduler::explanation::decision_reasons(decision),
                rejection_reasons:
                    crate::services::proxy::scheduler::explanation::rejection_reason_codes(decision),
                mapped_model: mapped_model.clone(),
                pricing_rule_id: economics.and_then(|economics| economics.pricing_rule_id.clone()),
                group_binding_id: scheduler_candidate.group_binding_id.clone(),
                rate_multiplier: decision
                    .effective_multiplier
                    .as_ref()
                    .map(|multiplier| multiplier.value),
                normalization_status: economics
                    .and_then(|economics| economics.normalization_status.clone()),
                price_confidence: economics.and_then(|economics| economics.price_confidence),
                estimated_input_price: economics.and_then(|economics| economics.estimated_input_price),
                estimated_output_price: economics
                    .and_then(|economics| economics.estimated_output_price),
                price_currency: economics.and_then(|economics| economics.price_currency.clone()),
                balance_status: economics.and_then(|economics| economics.balance_status.clone()),
                balance_value: economics.and_then(|economics| economics.balance_value),
                balance_scope: economics.and_then(|economics| economics.balance_scope.clone()),
                balance_collected_at: economics
                    .and_then(|economics| economics.balance_collected_at.clone()),
                economic_freshness: economics
                    .and_then(|economics| economics.economic_freshness.clone()),
                economic_reasons: Vec::new(),
                routing_group_scope: Some(routing_group_filter.clone()),
                routing_group_match: decision.routing_group_match,
                group_id_hash: scheduler_candidate.group_id_hash.clone(),
                group_type: scheduler_candidate.group_type.clone(),
                effective_multiplier_source: decision
                    .effective_multiplier
                    .as_ref()
                    .map(|multiplier| multiplier.source.clone()),
                effective_multiplier_confidence: decision
                    .effective_multiplier
                    .as_ref()
                    .map(|multiplier| multiplier.confidence),
                scheduler_score: decision.score,
                scheduler_factors: decision.factors.clone(),
                top_k_rank: decision.top_k_rank.map(|rank| rank as i64),
                slot_result: decision.slot_result.clone(),
            })
        })
        .collect()
}

fn insert_request_log_in_connection(
    connection: &Connection,
    input: CreateRequestLogInput,
) -> Result<RequestLog, String> {
    let id = generate_id("request");
    let created_at = now_string();
    let error_message = redact_optional_text(input.error_message);
    let route_reason = redact_optional_text(input.route_reason);
    let rejected_candidates_json = redact_optional_text(input.rejected_candidates_json);
    let economic_context_json = redact_optional_text(input.economic_context_json);
    connection
        .execute(
            "INSERT INTO request_logs (
                id, started_at, finished_at, duration_ms, method, path, model, stream,
                status, lifecycle_status, station_key_id, station_id, upstream_base_url,
                fallback_count, error_message, route_policy, route_reason, rejected_candidates_json,
                prompt_tokens, completion_tokens, total_tokens, cache_creation_tokens,
                cache_read_tokens, reasoning_effort, first_token_ms, billing_mode, estimated_input_cost,
                estimated_output_cost, estimated_total_cost, base_input_cost, base_output_cost,
                base_fixed_cost, base_total_cost, cost_currency, pricing_rule_id, pricing_source,
                cost_status, group_binding_id, normalization_status, balance_scope,
                economic_context_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42)",
            params![
                id,
                input.started_at,
                input.finished_at,
                input.duration_ms,
                input.method,
                input.path,
                normalize_optional_string(input.model),
                bool_to_i64(input.stream),
                input.status,
                normalize_optional_string(input.lifecycle_status),
                normalize_optional_string(input.station_key_id),
                normalize_optional_string(input.station_id),
                normalize_optional_string(input.upstream_base_url),
                input.fallback_count,
                error_message,
                normalize_optional_string(input.route_policy),
                route_reason,
                rejected_candidates_json,
                input.prompt_tokens,
                input.completion_tokens,
                input.total_tokens,
                input.cache_creation_tokens,
                input.cache_read_tokens,
                normalize_optional_string(input.reasoning_effort),
                input.first_token_ms,
                normalize_optional_string(input.billing_mode),
                input.estimated_input_cost,
                input.estimated_output_cost,
                input.estimated_total_cost,
                input.base_input_cost,
                input.base_output_cost,
                input.base_fixed_cost,
                input.base_total_cost,
                normalize_optional_string(input.cost_currency),
                normalize_optional_string(input.pricing_rule_id),
                normalize_optional_string(input.pricing_source),
                normalize_optional_string(input.cost_status),
                normalize_optional_string(input.group_binding_id),
                normalize_optional_string(input.normalization_status),
                normalize_optional_string(input.balance_scope),
                economic_context_json,
                created_at,
            ],
        )
        .map_err(|error| format!("保存请求日志失败: {error}"))?;

    let saved = request_log_by_id(connection, &id)?;
    if matches!(saved.status.as_str(), "failed" | "interrupted") {
        if let Some(station_id) = saved.station_id.as_deref() {
            let event = crate::services::change_events::route_impacted_event(
                station_id,
                saved
                    .model
                    .as_deref()
                    .or(saved.path.strip_prefix("/v1/"))
                    .unwrap_or("unknown"),
                saved
                    .route_reason
                    .as_deref()
                    .or(saved.error_message.as_deref())
                    .unwrap_or(if saved.status == "interrupted" {
                        "路由流式响应中断"
                    } else {
                        "路由请求失败"
                    }),
                Some(saved.id.as_str()),
            );
            let _ = upsert_change_event_in_connection(connection, event);
        }
    }
    Ok(saved)
}

fn timestamp_is_past(value: &str) -> bool {
    let Ok(timestamp) = value.trim().parse::<i64>() else {
        return false;
    };
    timestamp <= now_millis_for_services() as i64
}

fn row_to_request_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLog> {
    Ok(RequestLog {
        id: row.get(0)?,
        started_at: row.get(1)?,
        finished_at: row.get(2)?,
        duration_ms: row.get(3)?,
        method: row.get(4)?,
        path: row.get(5)?,
        model: row.get(6)?,
        stream: i64_to_bool(row.get(7)?),
        status: row.get(8)?,
        lifecycle_status: row.get(9)?,
        station_key_id: row.get(10)?,
        station_id: row.get(11)?,
        upstream_base_url: row.get(12)?,
        fallback_count: row.get(13)?,
        error_message: row.get(14)?,
        route_policy: row.get(15)?,
        route_reason: row.get(16)?,
        rejected_candidates_json: row.get(17)?,
        prompt_tokens: row.get(18)?,
        completion_tokens: row.get(19)?,
        total_tokens: row.get(20)?,
        cache_creation_tokens: row.get(21)?,
        cache_read_tokens: row.get(22)?,
        reasoning_effort: row.get(23)?,
        first_token_ms: row.get(24)?,
        billing_mode: row.get(25)?,
        estimated_input_cost: row.get(26)?,
        estimated_output_cost: row.get(27)?,
        estimated_total_cost: row.get(28)?,
        base_input_cost: row.get(29)?,
        base_output_cost: row.get(30)?,
        base_fixed_cost: row.get(31)?,
        base_total_cost: row.get(32)?,
        cost_currency: row.get(33)?,
        pricing_rule_id: row.get(34)?,
        pricing_source: row.get(35)?,
        cost_status: row.get(36)?,
        group_binding_id: row.get(37)?,
        normalization_status: row.get(38)?,
        balance_scope: row.get(39)?,
        economic_context_json: row.get(40)?,
        created_at: row.get(41)?,
    })
}

const REQUEST_LOG_SELECT_COLUMNS: &str = "
    id, started_at, finished_at, duration_ms, method, path, model, stream,
    status, lifecycle_status, station_key_id, station_id, upstream_base_url,
    fallback_count, error_message, route_policy, route_reason, rejected_candidates_json,
    prompt_tokens, completion_tokens, total_tokens, cache_creation_tokens,
    cache_read_tokens, reasoning_effort, first_token_ms, billing_mode, estimated_input_cost,
    estimated_output_cost, estimated_total_cost, base_input_cost, base_output_cost,
    base_fixed_cost, base_total_cost, cost_currency, pricing_rule_id, pricing_source,
    cost_status, group_binding_id, normalization_status, balance_scope,
    economic_context_json, created_at";

fn request_log_by_id(connection: &Connection, id: &str) -> Result<RequestLog, String> {
    connection
        .query_row(
            &format!("SELECT {REQUEST_LOG_SELECT_COLUMNS} FROM request_logs WHERE id = ?1"),
            params![id],
            row_to_request_log,
        )
        .optional()
        .map_err(|error| format!("读取请求日志失败: {error}"))?
        .ok_or_else(|| "请求日志不存在".to_string())
}

fn list_request_logs_from_connection(connection: &Connection) -> Result<Vec<RequestLog>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {REQUEST_LOG_SELECT_COLUMNS} FROM request_logs ORDER BY created_at DESC LIMIT 500"
        ))
        .map_err(|error| format!("读取请求日志列表失败: {error}"))?;

    let rows = statement
        .query_map([], row_to_request_log)
        .map_err(|error| format!("查询请求日志失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析请求日志失败: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|log| request_log_with_estimated_cost(connection, log))
        .collect())
}

fn list_local_proxy_request_logs_from_connection(
    connection: &Connection,
) -> Result<Vec<RequestLog>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {REQUEST_LOG_SELECT_COLUMNS}
             FROM request_logs
             WHERE COALESCE(route_policy, '') != 'channel_monitor'
             ORDER BY created_at DESC
             LIMIT 500"
        ))
        .map_err(|error| format!("读取本地路由日志列表失败: {error}"))?;

    let rows = statement
        .query_map([], row_to_request_log)
        .map_err(|error| format!("查询本地路由日志失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析本地路由日志失败: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|log| request_log_with_estimated_cost(connection, log))
        .collect())
}

fn request_log_with_estimated_cost(connection: &Connection, mut log: RequestLog) -> RequestLog {
    let has_cost_snapshot = log.cost_status.is_some()
        || log.estimated_input_cost.is_some()
        || log.estimated_output_cost.is_some()
        || log.estimated_total_cost.is_some();
    if log.cost_status.as_deref() == Some("usage_only")
        || (has_cost_snapshot && log.base_total_cost.is_some())
    {
        return log;
    }
    let Some(model) = log
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return log;
    };
    let Some(station_key_id) = log.station_key_id.as_deref() else {
        return log;
    };
    let Some(economics) =
        route_candidate_economics_by_station_key(connection, station_key_id, Some(model))
            .ok()
            .flatten()
    else {
        return log;
    };

    let estimate = crate::services::pricing::request_cost_from_pricing_parts(
        Some(crate::services::pricing::RequestPricingParts {
            station_key_id,
            station_id: log.station_id.as_deref(),
            model: Some(model),
            pricing_rule_id: economics.pricing_rule_id.as_deref(),
            pricing_model: economics.pricing_model.as_deref(),
            group_binding_id: economics.group_binding_id.as_deref(),
            rate_multiplier: economics.rate_multiplier,
            normalization_status: economics.normalization_status.as_deref(),
            price_confidence: economics.price_confidence,
            base_input_price: economics.base_input_price,
            base_output_price: economics.base_output_price,
            base_fixed_price: economics.base_fixed_price,
            estimated_input_price: economics.estimated_input_price,
            estimated_output_price: economics.estimated_output_price,
            fixed_price: economics.fixed_price,
            price_currency: economics.price_currency.as_deref(),
            pricing_source: economics.pricing_source.as_deref(),
            collected_at: economics.balance_collected_at.as_deref(),
        }),
        log.prompt_tokens,
        log.completion_tokens,
        log.total_tokens,
    );
    if estimate.estimated_total_cost.is_none() {
        return log;
    }

    if has_cost_snapshot {
        log.base_input_cost = estimate.base_input_cost;
        log.base_output_cost = estimate.base_output_cost;
        log.base_fixed_cost = estimate.base_fixed_cost;
        log.base_total_cost = estimate.base_total_cost;
        if log.group_binding_id.is_none() {
            log.group_binding_id = economics.group_binding_id;
        }
        if log.normalization_status.is_none() {
            log.normalization_status = economics.normalization_status;
        }
        if log.balance_scope.is_none() {
            log.balance_scope = economics.balance_scope;
        }
        return log;
    }

    log.estimated_input_cost = estimate.estimated_input_cost;
    log.estimated_output_cost = estimate.estimated_output_cost;
    log.estimated_total_cost = estimate.estimated_total_cost;
    log.base_input_cost = estimate.base_input_cost;
    log.base_output_cost = estimate.base_output_cost;
    log.base_fixed_cost = estimate.base_fixed_cost;
    log.base_total_cost = estimate.base_total_cost;
    log.cost_currency = estimate.cost_currency;
    log.pricing_rule_id = estimate.pricing_rule_id;
    log.pricing_source = estimate.pricing_source;
    log.cost_status = Some("legacy_estimate".to_string());
    if log.group_binding_id.is_none() {
        log.group_binding_id = economics.group_binding_id;
    }
    if log.normalization_status.is_none() {
        log.normalization_status = economics.normalization_status;
    }
    if log.balance_scope.is_none() {
        log.balance_scope = economics.balance_scope;
    }
    log
}

fn create_station_key_in_connection(
    connection: &Connection,
    input: CreateStationKeyInput,
) -> Result<StationKey, String> {
    create_station_key_in_connection_with_data_key(connection, input, None)
}

pub(super) fn create_station_key_in_connection_with_data_key(
    connection: &Connection,
    input: CreateStationKeyInput,
    data_key: Option<&[u8; 32]>,
) -> Result<StationKey, String> {
    validate_station_exists(connection, &input.station_id)?;
    if input.name.trim().is_empty() {
        return Err("Key 名称不能为空".to_string());
    }
    if input.api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }

    let id = generate_id("key");
    let now = now_string();
    let plaintext_api_key = input.api_key.trim().to_string();
    let stored_api_key = if data_key.is_some() {
        "".to_string()
    } else {
        plaintext_api_key.clone()
    };
    let secret_id = if let Some(data_key) = data_key {
        Some(upsert_secret_in_connection(
            connection,
            data_key,
            "station_key",
            &id,
            "api_key",
            &plaintext_api_key,
        )?)
    } else {
        None
    };
    let priority = match input.priority {
        Some(priority) => priority,
        None => next_station_key_priority(connection, &input.station_id)?,
    };
    let max_concurrency = input.max_concurrency.unwrap_or(3);
    let load_factor = input.load_factor;
    let schedulable = input.schedulable.unwrap_or(input.enabled);
    validate_station_key_scheduler_values(
        max_concurrency,
        load_factor,
        input.manual_rate_multiplier,
    )?;
    let routing_order = next_local_routing_order(connection)?;

    connection
        .execute(
            "INSERT INTO station_keys (
                id, station_id, name, api_key, api_key_secret_id, enabled, priority, routing_order,
                max_concurrency, load_factor, schedulable,
                group_name, tier_label, group_binding_id, group_id_hash, rate_multiplier,
                manual_rate_multiplier, manual_rate_updated_at,
                rate_source, rate_collected_at, balance_scope,
                status, last_checked_at, last_used_at, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 'unchecked', NULL, NULL, ?22, ?23, ?24)",
            params![
                id,
                input.station_id,
                input.name.trim(),
                stored_api_key,
                secret_id,
                bool_to_i64(input.enabled),
                priority,
                routing_order,
                max_concurrency,
                load_factor,
                bool_to_i64(schedulable),
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
                normalize_optional_string(input.group_binding_id),
                normalize_optional_string(input.group_id_hash),
                input.rate_multiplier,
                input.manual_rate_multiplier,
                input.manual_rate_multiplier.map(|_| now.clone()),
                normalize_optional_string(input.rate_source),
                input.rate_multiplier.map(|_| now.clone()),
                normalize_optional_string(input.balance_scope),
                normalize_optional_string(input.note),
                now,
                now,
            ],
        )
        .map_err(|error| format!("创建 Station Key 失败: {error}"))?;

    station_key_by_id(connection, &id)
}

fn update_station_key_in_connection(
    connection: &Connection,
    input: UpdateStationKeyInput,
) -> Result<StationKey, String> {
    update_station_key_in_connection_with_data_key(connection, input, None)
}

pub(super) fn update_station_key_in_connection_with_data_key(
    connection: &Connection,
    input: UpdateStationKeyInput,
    data_key: Option<&[u8; 32]>,
) -> Result<StationKey, String> {
    if input.name.trim().is_empty() {
        return Err("Key 名称不能为空".to_string());
    }

    let existing: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT api_key, api_key_secret_id FROM station_keys WHERE id = ?1 AND station_id = ?2",
            params![input.id, input.station_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?;

    let Some((existing_api_key, existing_secret_id)) = existing else {
        return Err("Station Key 不存在，无法更新".to_string());
    };

    let new_api_key = input
        .api_key
        .as_ref()
        .map(|api_key| api_key.trim())
        .filter(|api_key| !api_key.is_empty())
        .map(ToString::to_string);
    let (next_api_key, next_secret_id) = if let Some(data_key) = data_key {
        let secret_id = match new_api_key {
            Some(api_key) => Some(upsert_secret_in_connection(
                connection,
                data_key,
                "station_key",
                &input.id,
                "api_key",
                &api_key,
            )?),
            None => existing_secret_id,
        };
        ("".to_string(), secret_id)
    } else {
        (new_api_key.unwrap_or(existing_api_key), existing_secret_id)
    };
    let manual_rate_multiplier_update = input.manual_rate_multiplier;
    let (manual_rate_multiplier_present, manual_rate_multiplier) =
        match manual_rate_multiplier_update {
            Some(value) => (1_i64, value),
            None => (0_i64, None),
        };
    validate_station_key_scheduler_values(
        input.max_concurrency,
        input.load_factor,
        manual_rate_multiplier,
    )?;
    let now = now_string();

    connection
        .execute(
            "UPDATE station_keys
                SET name = ?1,
                    api_key = ?2,
                    api_key_secret_id = ?3,
                    enabled = ?4,
                    priority = ?5,
                    max_concurrency = ?6,
                    load_factor = ?7,
                    schedulable = ?8,
                    group_name = ?9,
                    tier_label = ?10,
                    group_binding_id = COALESCE(?11, group_binding_id),
                    group_id_hash = COALESCE(?12, group_id_hash),
                    rate_multiplier = COALESCE(?13, rate_multiplier),
                    manual_rate_multiplier = CASE
                        WHEN ?14 = 1 THEN ?15
                        ELSE manual_rate_multiplier
                    END,
                    manual_rate_updated_at = CASE
                        WHEN ?14 = 1 THEN ?16
                        ELSE manual_rate_updated_at
                    END,
                    rate_source = COALESCE(?17, rate_source),
                    rate_collected_at = CASE
                        WHEN ?13 IS NOT NULL THEN ?16
                        ELSE rate_collected_at
                    END,
                    balance_scope = COALESCE(?18, balance_scope),
                    status = ?19,
                    note = ?20,
                    updated_at = ?21
              WHERE id = ?22 AND station_id = ?23",
            params![
                input.name.trim(),
                next_api_key,
                next_secret_id,
                bool_to_i64(input.enabled),
                input.priority,
                input.max_concurrency,
                input.load_factor,
                bool_to_i64(input.schedulable),
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
                normalize_optional_string(input.group_binding_id),
                normalize_optional_string(input.group_id_hash),
                input.rate_multiplier,
                manual_rate_multiplier_present,
                manual_rate_multiplier,
                now.clone(),
                normalize_optional_string(input.rate_source),
                normalize_optional_string(input.balance_scope),
                input.status,
                normalize_optional_string(input.note),
                now,
                input.id,
                input.station_id,
            ],
        )
        .map_err(|error| format!("更新 Station Key 失败: {error}"))?;

    station_key_by_id(connection, &input.id)
}

pub(super) fn update_station_key_group_binding_in_connection(
    connection: &Connection,
    input: UpdateStationKeyGroupBindingInput,
) -> Result<StationKey, String> {
    validate_station_key_exists(connection, &input.station_key_id)?;
    let key = station_key_by_id(connection, &input.station_key_id)?;
    let binding = station_group_binding_by_id(connection, &input.group_binding_id)?;
    if binding.station_id != key.station_id {
        return Err("分组绑定不属于该 Key 的中转站".to_string());
    }
    if let Some(bound_key_id) = binding.station_key_id.as_deref() {
        if bound_key_id != input.station_key_id {
            return Err("分组绑定已属于其他 Key".to_string());
        }
    }

    let now = now_string();
    connection
        .execute(
            "UPDATE station_keys
                SET group_binding_id = ?1,
                    group_id_hash = ?2,
                    group_name = ?3,
                    rate_multiplier = ?4,
                    rate_source = 'manual',
                    rate_collected_at = ?5,
                    balance_scope = COALESCE(balance_scope, 'station'),
                    updated_at = ?5
              WHERE id = ?6",
            params![
                &binding.id,
                &binding.group_key_hash,
                &binding.group_name,
                binding.effective_rate_multiplier,
                &now,
                &input.station_key_id,
            ],
        )
        .map_err(|error| format!("更新 Key 分组绑定失败: {error}"))?;

    let event = crate::services::change_events::key_group_bound_event(
        &binding.station_id,
        &input.station_key_id,
        &binding.id,
        &binding.group_name,
    );
    let _ = upsert_change_event_in_connection(connection, event);

    station_key_by_id(connection, &input.station_key_id)
}

pub(super) fn clear_station_key_group_binding_in_connection(
    connection: &Connection,
    station_key_id: &str,
) -> Result<StationKey, String> {
    validate_station_key_exists(connection, station_key_id)?;
    let now = now_string();
    connection
        .execute(
            "UPDATE station_keys
                SET group_binding_id = NULL,
                    group_id_hash = NULL,
                    group_name = NULL,
                    rate_multiplier = NULL,
                    rate_source = NULL,
                    rate_collected_at = NULL,
                    updated_at = ?1
              WHERE id = ?2",
            params![now, station_key_id],
        )
        .map_err(|error| format!("清除 Key 分组绑定失败: {error}"))?;

    station_key_by_id(connection, station_key_id)
}

fn touch_station_key_usage_in_connection(
    connection: &Connection,
    station_key_id: &str,
    status: &str,
    last_used_at: Option<&str>,
    last_checked_at: Option<&str>,
) -> Result<(), String> {
    let now = now_string();
    let updated = connection
        .execute(
            "UPDATE station_keys
                SET status = ?1,
                    last_used_at = COALESCE(?2, last_used_at),
                    last_checked_at = COALESCE(?3, last_checked_at),
                    updated_at = ?4
              WHERE id = ?5",
            params![status, last_used_at, last_checked_at, now, station_key_id],
        )
        .map_err(|error| format!("更新 Station Key 使用状态失败: {error}"))?;

    if updated == 0 {
        return Err("Station Key 不存在，无法更新状态".to_string());
    }

    Ok(())
}

pub(super) fn station_key_by_id(connection: &Connection, id: &str) -> Result<StationKey, String> {
    connection
        .query_row(
            "SELECT id, station_id, name, api_key, enabled, priority,
                    max_concurrency, load_factor, schedulable,
                    group_name, tier_label,
                    group_binding_id, group_id_hash, rate_multiplier,
                    manual_rate_multiplier, manual_rate_updated_at, rate_source,
                    rate_collected_at, balance_scope,
                    status, last_checked_at, last_used_at, note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = station_keys.api_key_secret_id),
                    api_key_secret_id
               FROM station_keys
              WHERE id = ?1",
            params![id],
            row_to_station_key,
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?
        .ok_or_else(|| "Station Key 不存在".to_string())
}

fn next_station_key_priority(connection: &Connection, station_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(priority), -1) + 1 FROM station_keys WHERE station_id = ?1",
            params![station_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("计算 Key 排序失败: {error}"))
}

fn next_local_routing_order(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(routing_order), -1) + 1 FROM station_keys",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Calculate local routing order failed: {error}"))
}

fn normalize_station_key_priorities(
    connection: &Connection,
    station_id: &str,
) -> Result<(), String> {
    let ids = {
        let mut statement = connection
            .prepare(
                "SELECT id FROM station_keys WHERE station_id = ?1 ORDER BY priority ASC, created_at ASC",
            )
            .map_err(|error| format!("读取 Key 排序失败: {error}"))?;
        let rows = statement
            .query_map(params![station_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询 Key 排序失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("解析 Key 排序失败: {error}"))?;
        rows
    };

    for (index, id) in ids.iter().enumerate() {
        connection
            .execute(
                "UPDATE station_keys SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                params![index as i64, now_string(), id],
            )
            .map_err(|error| format!("整理 Key 排序失败: {error}"))?;
    }

    Ok(())
}

fn station_credentials_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<StationCredentials, String> {
    let credentials = connection
        .query_row(
            "SELECT station_id, login_username, login_password, login_password_secret_id,
                    access_token_secret_id, refresh_token_secret_id, cookie_secret_id,
                    remember_password,
                    login_status, login_error, last_login_at, session_status,
                    session_expires_at, newapi_user_id, token_expires_at,
                    token_refreshed_at, session_source, updated_at
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                let password: Option<String> = row.get(2)?;
                let password_secret_id: Option<String> = row.get(3)?;
                let access_token_secret_id: Option<String> = row.get(4)?;
                let refresh_token_secret_id: Option<String> = row.get(5)?;
                let cookie_secret_id: Option<String> = row.get(6)?;
                Ok(StationCredentials {
                    station_id: row.get(0)?,
                    login_username: row.get(1)?,
                    password_present: password_secret_id.is_some()
                        || password
                            .map(|value| !value.trim().is_empty())
                            .unwrap_or(false),
                    access_token_present: access_token_secret_id.is_some(),
                    refresh_token_present: refresh_token_secret_id.is_some(),
                    cookie_present: cookie_secret_id.is_some(),
                    remember_password: i64_to_bool(row.get(7)?),
                    login_status: row.get(8)?,
                    login_error: row.get(9)?,
                    last_login_at: row.get(10)?,
                    session_status: row.get(11)?,
                    session_expires_at: row.get(12)?,
                    newapi_user_id: row.get(13)?,
                    token_expires_at: row.get(14)?,
                    token_refreshed_at: row.get(15)?,
                    session_source: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取登录信息失败: {error}"))?;

    Ok(credentials.unwrap_or_else(|| StationCredentials {
        station_id: station_id.to_string(),
        login_username: None,
        password_present: false,
        access_token_present: false,
        refresh_token_present: false,
        cookie_present: false,
        remember_password: false,
        login_status: "unknown".to_string(),
        login_error: None,
        last_login_at: None,
        session_status: "none".to_string(),
        session_expires_at: None,
        newapi_user_id: None,
        token_expires_at: None,
        token_refreshed_at: None,
        session_source: "none".to_string(),
        updated_at: None,
    }))
}

fn station_login_password_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Option<String>, String> {
    station_login_password_from_connection_with_data_key(connection, station_id, None)
}

fn station_login_password_from_connection_with_data_key(
    connection: &Connection,
    station_id: &str,
    data_key: Option<&[u8; 32]>,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT login_password, login_password_secret_id
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取登录密码失败: {error}"))?
        .map(|(password, secret_id)| {
            if let Some(secret_id) = secret_id {
                let Some(data_key) = data_key else {
                    return Err("登录密码已迁移为加密凭据，当前调用缺少解密密钥".to_string());
                };
                return decrypt_secret_by_id(connection, data_key, &secret_id).map(Some);
            }
            Ok(password.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }))
        })
        .ok_or_else(|| "未找到登录信息".to_string())
        .and_then(|result| result)
}

fn resolve_station_session_from_connection(
    connection: &Connection,
    station_id: &str,
    data_key: &[u8; 32],
    now_ms: i64,
) -> Result<ResolvedSession, String> {
    let row = connection
        .query_row(
            "SELECT access_token_secret_id, refresh_token_secret_id, cookie_secret_id,
                    newapi_user_id, token_expires_at, session_source,
                    login_username, login_password, login_password_secret_id
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取 session 凭据失败: {error}"))?;

    let Some((
        access_token_secret_id,
        refresh_token_secret_id,
        cookie_secret_id,
        newapi_user_id,
        token_expires_at,
        session_source,
        login_username,
        login_password,
        login_password_secret_id,
    )) = row
    else {
        return Ok(ResolvedSession::manual_required("缺少 session 凭据。"));
    };

    let access_token = decrypt_optional_secret(connection, data_key, access_token_secret_id)?;
    let refresh_token = decrypt_optional_secret(connection, data_key, refresh_token_secret_id)?;
    let cookie = decrypt_optional_secret(connection, data_key, cookie_secret_id)?;
    let password_present = login_password_secret_id.is_some()
        || login_password
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    let password_login_available = login_username
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        && password_present;

    if access_token.is_some()
        && (token_is_fresh(token_expires_at.as_deref(), now_ms)
            || (token_expires_at.is_none()
                && (session_source == "manual_token" || session_source == "webview_capture")))
    {
        return Ok(ResolvedSession {
            status: SessionResolveStatus::Ready,
            access_token,
            refresh_token,
            cookie,
            newapi_user_id,
            message: None,
        });
    }

    if cookie.is_some()
        && newapi_user_id
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    {
        return Ok(ResolvedSession {
            status: SessionResolveStatus::Ready,
            access_token,
            refresh_token,
            cookie,
            newapi_user_id,
            message: None,
        });
    }

    if refresh_token.is_some() || password_login_available {
        return Ok(ResolvedSession {
            status: SessionResolveStatus::Ready,
            access_token,
            refresh_token,
            cookie,
            newapi_user_id,
            message: Some("需要刷新 token 或重新登录。".to_string()),
        });
    }

    Ok(ResolvedSession::manual_required(
        "缺少可用 access token、refresh token 或登录凭据。",
    ))
}

fn decrypt_optional_secret(
    connection: &Connection,
    data_key: &[u8; 32],
    secret_id: Option<String>,
) -> Result<Option<String>, String> {
    match secret_id {
        Some(secret_id) => decrypt_secret_by_id(connection, data_key, &secret_id).map(Some),
        None => Ok(None),
    }
}

fn upsert_station_credentials(
    connection: &Connection,
    input: UpdateStationCredentialsInput,
) -> Result<(), String> {
    upsert_station_credentials_with_data_key(connection, input, None)
}

fn upsert_station_credentials_with_data_key(
    connection: &Connection,
    input: UpdateStationCredentialsInput,
    data_key: Option<&[u8; 32]>,
) -> Result<(), String> {
    let existing: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT login_password, login_password_secret_id FROM station_credentials WHERE station_id = ?1",
            params![input.station_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取旧密码失败: {error}"))?
        .unwrap_or((None, None));

    let new_password = if input.remember_password {
        input
            .login_password
            .as_ref()
            .map(|password| password.trim().to_string())
            .filter(|password| !password.is_empty())
    } else {
        None
    };
    let (password, password_secret_id) = if input.remember_password {
        if let Some(data_key) = data_key {
            let secret_id = match new_password {
                Some(password) => Some(upsert_secret_in_connection(
                    connection,
                    data_key,
                    "station",
                    &input.station_id,
                    "login_password",
                    &password,
                )?),
                None => existing.1,
            };
            (None, secret_id)
        } else {
            (new_password.or(existing.0), existing.1)
        }
    } else {
        (None, None)
    };
    let now = now_string();

    connection
        .execute(
            "INSERT INTO station_credentials (
                id, station_id, login_username, login_password, login_password_secret_id, remember_password,
                login_status, login_error, last_login_at, session_status,
                session_expires_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'saved', NULL, NULL, 'none', NULL, ?7, ?8)
             ON CONFLICT(station_id) DO UPDATE SET
                login_username = excluded.login_username,
                login_password = excluded.login_password,
                login_password_secret_id = excluded.login_password_secret_id,
                remember_password = excluded.remember_password,
                login_status = 'saved',
                login_error = NULL,
                updated_at = excluded.updated_at",
            params![
                generate_id("credentials"),
                input.station_id,
                normalize_optional_string(input.login_username),
                password,
                password_secret_id,
                bool_to_i64(input.remember_password),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存登录信息失败: {error}"))?;

    Ok(())
}

fn upsert_station_session_with_data_key(
    connection: &Connection,
    input: UpdateStationSessionInput,
    data_key: &[u8; 32],
) -> Result<(), String> {
    persist_station_session_from_connection(
        connection,
        PersistStationSessionInput {
            station_id: input.station_id,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
            cookie: input.cookie,
            newapi_user_id: input.newapi_user_id,
            session_expires_at: input.token_expires_at.clone(),
            token_expires_at: input.token_expires_at,
            session_source: "manual_token".to_string(),
        },
        data_key,
    )
}

fn persist_station_session_from_connection(
    connection: &Connection,
    input: PersistStationSessionInput,
    data_key: &[u8; 32],
) -> Result<(), String> {
    let access_token = normalize_optional_string(input.access_token);
    let refresh_token = normalize_optional_string(input.refresh_token);
    let cookie = normalize_optional_string(input.cookie);
    let newapi_user_id = normalize_optional_string(input.newapi_user_id);
    let token_expires_at = normalize_optional_string(input.token_expires_at);
    let session_expires_at = normalize_optional_string(input.session_expires_at);
    let session_source = normalize_required_string(input.session_source, "manual_token");

    let existing: (Option<String>, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT access_token_secret_id, refresh_token_secret_id, cookie_secret_id
               FROM station_credentials
              WHERE station_id = ?1",
            params![input.station_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("读取旧 session 凭据失败: {error}"))?
        .unwrap_or((None, None, None));

    let access_token_secret_id = match access_token {
        Some(value) => Some(upsert_secret_in_connection(
            connection,
            data_key,
            "station",
            &input.station_id,
            "access_token",
            &value,
        )?),
        None => existing.0,
    };
    let refresh_token_secret_id = match refresh_token {
        Some(value) => Some(upsert_secret_in_connection(
            connection,
            data_key,
            "station",
            &input.station_id,
            "refresh_token",
            &value,
        )?),
        None => existing.1,
    };
    let cookie_secret_id = match cookie {
        Some(value) => Some(upsert_secret_in_connection(
            connection,
            data_key,
            "station",
            &input.station_id,
            "cookie",
            &value,
        )?),
        None => existing.2,
    };
    let has_session = access_token_secret_id.is_some()
        || refresh_token_secret_id.is_some()
        || cookie_secret_id.is_some();
    let now = now_string();

    connection
        .execute(
            "INSERT INTO station_credentials (
                id, station_id, remember_password, login_status, session_status,
                session_expires_at, access_token_secret_id, refresh_token_secret_id,
                cookie_secret_id, newapi_user_id, token_expires_at, token_refreshed_at,
                session_source, created_at, updated_at
             ) VALUES (?1, ?2, 0, 'saved', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(station_id) DO UPDATE SET
                session_status = excluded.session_status,
                session_expires_at = excluded.session_expires_at,
                access_token_secret_id = excluded.access_token_secret_id,
                refresh_token_secret_id = excluded.refresh_token_secret_id,
                cookie_secret_id = excluded.cookie_secret_id,
                newapi_user_id = excluded.newapi_user_id,
                token_expires_at = excluded.token_expires_at,
                token_refreshed_at = excluded.token_refreshed_at,
                session_source = excluded.session_source,
                updated_at = excluded.updated_at",
            params![
                generate_id("credentials"),
                input.station_id,
                if has_session {
                    "valid"
                } else {
                    "manual_required"
                },
                session_expires_at,
                access_token_secret_id,
                refresh_token_secret_id,
                cookie_secret_id,
                newapi_user_id,
                token_expires_at,
                now,
                session_source,
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存 session 凭据失败: {error}"))?;

    Ok(())
}

fn invalidate_station_session_credential_from_connection(
    connection: &Connection,
    station_id: &str,
    kind: StationSessionCredentialKind,
) -> Result<(), String> {
    let (column, label) = match kind {
        StationSessionCredentialKind::AccessToken => ("access_token_secret_id", "access token"),
        StationSessionCredentialKind::RefreshToken => ("refresh_token_secret_id", "refresh token"),
        StationSessionCredentialKind::Cookie => ("cookie_secret_id", "Cookie"),
    };
    let secret_id: Option<String> = connection
        .query_row(
            &format!("SELECT {column} FROM station_credentials WHERE station_id = ?1"),
            params![station_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取待失效 {label} 凭据失败: {error}"))?
        .flatten();

    connection
        .execute(
            &format!(
                "UPDATE station_credentials
                    SET {column} = NULL,
                        updated_at = ?1
                  WHERE station_id = ?2"
            ),
            params![now_string(), station_id],
        )
        .map_err(|error| format!("失效 {label} 凭据失败: {error}"))?;
    refresh_station_session_status_from_connection(connection, station_id)?;
    if let Some(secret_id) = secret_id {
        delete_unreferenced_secret_by_id(connection, &secret_id)?;
    }
    Ok(())
}

fn refresh_station_session_status_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<(), String> {
    let has_session = connection
        .query_row(
            "SELECT access_token_secret_id IS NOT NULL
                    OR refresh_token_secret_id IS NOT NULL
                    OR cookie_secret_id IS NOT NULL
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| format!("读取 session 状态失败: {error}"))?
        .map(i64_to_bool)
        .unwrap_or(false);
    connection
        .execute(
            "UPDATE station_credentials
                SET session_status = ?1,
                    updated_at = ?2
              WHERE station_id = ?3",
            params![
                if has_session {
                    "valid"
                } else {
                    "manual_required"
                },
                now_string(),
                station_id
            ],
        )
        .map_err(|error| format!("更新 session 状态失败: {error}"))?;
    Ok(())
}

fn delete_unreferenced_secret_by_id(
    connection: &Connection,
    secret_id: &str,
) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM secrets
              WHERE id = ?1
                AND NOT EXISTS (
                    SELECT 1 FROM station_credentials
                     WHERE login_password_secret_id = ?1
                        OR access_token_secret_id = ?1
                        OR refresh_token_secret_id = ?1
                        OR cookie_secret_id = ?1
                )
                AND NOT EXISTS (
                    SELECT 1 FROM station_keys WHERE api_key_secret_id = ?1
                )
                AND NOT EXISTS (
                    SELECT 1 FROM stations WHERE api_key_secret_id = ?1
                )",
            params![secret_id],
        )
        .map_err(|error| format!("删除未引用加密凭据失败: {error}"))?;
    Ok(())
}

fn update_station_login_status_in_connection(
    connection: &Connection,
    station_id: &str,
    login_status: &str,
    login_error: Option<String>,
) -> Result<(), String> {
    let now = now_string();
    connection
        .execute(
            "INSERT INTO station_credentials (
                id, station_id, remember_password, login_status, login_error,
                session_status, created_at, updated_at
             ) VALUES (?1, ?2, 0, ?3, ?4, 'none', ?5, ?6)
             ON CONFLICT(station_id) DO UPDATE SET
                login_status = excluded.login_status,
                login_error = excluded.login_error,
                updated_at = excluded.updated_at",
            params![
                generate_id("credentials"),
                station_id,
                login_status,
                normalize_optional_string(login_error),
                now,
                now,
            ],
        )
        .map_err(|error| format!("更新登录状态失败: {error}"))?;
    Ok(())
}

fn insert_collector_snapshot_in_connection(
    connection: &Connection,
    station_id: &str,
    source: &str,
    status: &str,
    summary_json: Value,
    normalized_json: Value,
    raw_json_redacted: Option<Value>,
    error_message: Option<String>,
) -> Result<CollectorSnapshot, String> {
    let endpoint_revision = station_endpoint_revision(connection, station_id)?;
    insert_collector_snapshot_in_connection_with_revision(
        connection,
        station_id,
        endpoint_revision,
        source,
        status,
        summary_json,
        normalized_json,
        raw_json_redacted,
        error_message,
        true,
    )
}

pub(crate) fn insert_collector_snapshot_in_connection_with_revision(
    connection: &Connection,
    station_id: &str,
    endpoint_revision: i64,
    source: &str,
    status: &str,
    summary_json: Value,
    normalized_json: Value,
    raw_json_redacted: Option<Value>,
    error_message: Option<String>,
    emit_change_events: bool,
) -> Result<CollectorSnapshot, String> {
    let id = generate_id("snapshot");
    let now = now_string();
    validate_station_exists(connection, station_id)?;
    let summary_json = redact_sensitive_value(&summary_json);
    let normalized_json = redact_sensitive_value(&normalized_json);
    let raw_json_redacted = raw_json_redacted.map(|value| redact_sensitive_value(&value));
    let error_message = redact_optional_text(error_message);
    let previous_snapshot = latest_collector_snapshot_from_connection(connection, station_id)?;
    let raw_json_string = raw_json_redacted
        .as_ref()
        .map(|value| serde_json::to_string(value))
        .transpose()
        .map_err(|error| format!("序列化脱敏 raw 失败: {error}"))?;
    connection
        .execute(
            "INSERT INTO collector_snapshots (
                id, station_id, endpoint_revision, source, status, fetched_at, summary_json,
                normalized_json, raw_json_redacted, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                station_id,
                endpoint_revision,
                source,
                status,
                now,
                serde_json::to_string(&summary_json)
                    .map_err(|error| format!("序列化 summary 失败: {error}"))?,
                serde_json::to_string(&normalized_json)
                    .map_err(|error| format!("序列化 normalized 失败: {error}"))?,
                raw_json_string,
                error_message,
                now,
            ],
        )
        .map_err(|error| format!("保存采集快照失败: {error}"))?;

    let saved = collector_snapshot_by_id(connection, &id)?;
    if emit_change_events && saved.status == "failed" {
        let task_type = collector_task_type_from_snapshot_source(&saved.source);
        let event = crate::services::change_events::collector_failed_event(
            &saved.station_id,
            &task_type,
            saved.error_message.as_deref(),
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
    if emit_change_events {
        if let Some(previous_snapshot) = previous_snapshot.as_ref() {
            let previous_models = models_from_snapshot_value(&previous_snapshot.normalized_json);
            let next_models = models_from_snapshot_value(&saved.normalized_json);
            if should_emit_model_change_events(&saved.source) {
                for model in next_models
                    .iter()
                    .filter(|model| !previous_models.contains(model))
                {
                    let event = UpsertChangeEventInput {
                        severity: crate::services::change_events::SEVERITY_INFO.to_string(),
                        event_type: "model_added".to_string(),
                        title: "模型新增".to_string(),
                        message: format!("站点新增模型 {model}"),
                        object_type: "station".to_string(),
                        object_id: Some(saved.station_id.clone()),
                        station_id: Some(saved.station_id.clone()),
                        station_key_id: None,
                        pricing_rule_id: None,
                        request_log_id: None,
                        old_value_json: None,
                        new_value_json: Some(json!({ "model": model }).to_string()),
                        impact_json: None,
                        dedupe_key: crate::services::change_events::model_dedupe_key(
                            &saved.station_id,
                            "model_added",
                            model,
                        ),
                        source: "collector".to_string(),
                    };
                    let _ = upsert_change_event_in_connection(connection, event);
                }
                for model in previous_models
                    .iter()
                    .filter(|model| !next_models.contains(model))
                {
                    let event = UpsertChangeEventInput {
                        severity: crate::services::change_events::SEVERITY_WARNING.to_string(),
                        event_type: "model_removed".to_string(),
                        title: "模型下架".to_string(),
                        message: format!("站点下架模型 {model}"),
                        object_type: "station".to_string(),
                        object_id: Some(saved.station_id.clone()),
                        station_id: Some(saved.station_id.clone()),
                        station_key_id: None,
                        pricing_rule_id: None,
                        request_log_id: None,
                        old_value_json: Some(json!({ "model": model }).to_string()),
                        new_value_json: None,
                        impact_json: Some(
                            json!({ "routingRisk": "model_candidates_may_change" }).to_string(),
                        ),
                        dedupe_key: crate::services::change_events::model_dedupe_key(
                            &saved.station_id,
                            "model_removed",
                            model,
                        ),
                        source: "collector".to_string(),
                    };
                    let _ = upsert_change_event_in_connection(connection, event);
                }
            }

            let previous_rates =
                rate_multipliers_from_snapshot_value(&previous_snapshot.normalized_json);
            let next_rates = rate_multipliers_from_snapshot_value(&saved.normalized_json);
            for (group_name, next_multiplier) in next_rates {
                if let Some((_, old_multiplier)) = previous_rates
                    .iter()
                    .find(|(previous_group, _)| previous_group == &group_name)
                {
                    if let Some(event) = crate::services::change_events::rate_changed_event(
                        &saved.station_id,
                        &group_name,
                        *old_multiplier,
                        next_multiplier,
                    ) {
                        let _ = upsert_change_event_in_connection(connection, event);
                    }
                }
            }
        }
    }
    Ok(saved)
}

fn should_emit_model_change_events(source: &str) -> bool {
    source != "newapi-models"
}

fn row_to_collector_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectorSnapshot> {
    let summary_string: String = row.get("summary_json")?;
    let normalized_string: String = row.get("normalized_json")?;
    let raw_string: Option<String> = row.get("raw_json_redacted")?;

    Ok(CollectorSnapshot {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        endpoint_revision: row.get("endpoint_revision")?,
        source: row.get("source")?,
        status: row.get("status")?,
        fetched_at: row.get("fetched_at")?,
        summary_json: parse_json_value(&summary_string),
        normalized_json: parse_json_value(&normalized_string),
        raw_json_redacted: raw_string.as_deref().map(parse_json_value),
        error_message: row.get("error_message")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_station_group_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<StationGroupBinding> {
    let raw_json: Option<String> = row.get("raw_json_redacted")?;
    Ok(StationGroupBinding {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        station_key_id: row.get("station_key_id")?,
        binding_kind: row.get("binding_kind")?,
        parent_group_binding_id: row.get("parent_group_binding_id")?,
        group_key_hash: row.get("group_key_hash")?,
        group_id_hash: row.get("group_id_hash")?,
        group_name: row.get("group_name")?,
        binding_status: row.get("binding_status")?,
        default_rate_multiplier: row.get("default_rate_multiplier")?,
        user_rate_multiplier: row.get("user_rate_multiplier")?,
        effective_rate_multiplier: row.get("effective_rate_multiplier")?,
        inferred_group_category: row.get("inferred_group_category")?,
        group_category_override: row.get("group_category_override")?,
        rate_source: row.get("rate_source")?,
        confidence: row.get("confidence")?,
        last_seen_at: row.get("last_seen_at")?,
        last_checked_at: row.get("last_checked_at")?,
        last_rate_changed_at: row.get("last_rate_changed_at")?,
        raw_json_redacted: raw_json.as_deref().map(parse_json_value),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_group_rate_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<GroupRateRecord> {
    let raw_json: Option<String> = row.get("raw_json_redacted")?;
    Ok(GroupRateRecord {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        station_key_id: row.get("station_key_id")?,
        group_binding_id: row.get("group_binding_id")?,
        binding_kind: row.get("binding_kind")?,
        group_key_hash: row.get("group_key_hash")?,
        group_name: row.get("group_name")?,
        default_rate_multiplier: row.get("default_rate_multiplier")?,
        user_rate_multiplier: row.get("user_rate_multiplier")?,
        effective_rate_multiplier: row.get("effective_rate_multiplier")?,
        inferred_group_category: row.get("inferred_group_category")?,
        source: row.get("source")?,
        confidence: row.get("confidence")?,
        raw_json_redacted: raw_json.as_deref().map(parse_json_value),
        checked_at: row.get("checked_at")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_collector_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectorRun> {
    Ok(CollectorRun {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        endpoint_revision: row.get("endpoint_revision")?,
        parent_run_id: row.get("parent_run_id")?,
        adapter: row.get("adapter")?,
        task_type: row.get("task_type")?,
        status: row.get("status")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        duration_ms: row.get("duration_ms")?,
        endpoint_count: row.get("endpoint_count")?,
        success_count: row.get("success_count")?,
        failure_count: row.get("failure_count")?,
        manual_action_required: i64_to_bool(row.get("manual_action_required")?),
        error_code: row.get("error_code")?,
        error_message: row.get("error_message")?,
        snapshot_id: row.get("snapshot_id")?,
        created_at: row.get("created_at")?,
    })
}

fn validate_binding_kind(value: &str) -> Result<String, String> {
    match value.trim() {
        "station_group" | "key_binding" => Ok(value.trim().to_string()),
        _ => Err("分组绑定类型无效".to_string()),
    }
}

fn validate_binding_status(value: &str) -> Result<String, String> {
    match value.trim() {
        "available" | "bound" | "missing" | "disabled" | "manual_legacy" => {
            Ok(value.trim().to_string())
        }
        _ => Err("分组绑定状态无效".to_string()),
    }
}

fn validate_non_empty_hash(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err("group_key_hash 不能为空".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn stable_group_key_hash(
    station_id: &str,
    adapter: &str,
    group_id: Option<&str>,
    group_name: &str,
) -> String {
    let adapter = adapter.trim().to_lowercase();
    let source = if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        format!("id:{adapter}:{}", group_id.trim())
    } else {
        format!(
            "name:{}:{}:{}",
            station_id,
            adapter,
            group_name.trim().to_lowercase()
        )
    };
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn existing_group_binding_id(
    connection: &Connection,
    station_id: &str,
    station_key_id: Option<&str>,
    binding_kind: &str,
    group_key_hash: &str,
) -> Result<Option<String>, String> {
    if binding_kind == "key_binding" {
        let Some(station_key_id) = station_key_id else {
            return Err("Key 分组绑定必须关联 Station Key".to_string());
        };
        return connection
            .query_row(
                "SELECT id FROM station_group_bindings
                 WHERE station_key_id = ?1 AND binding_kind = ?2 AND group_key_hash = ?3
                 LIMIT 1",
                params![station_key_id, binding_kind, group_key_hash],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("读取已有 Key 分组绑定失败: {error}"));
    }

    connection
        .query_row(
            "SELECT id FROM station_group_bindings
             WHERE station_id = ?1 AND binding_kind = ?2 AND group_key_hash = ?3
             LIMIT 1",
            params![station_id, binding_kind, group_key_hash],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取已有站点分组绑定失败: {error}"))
}

fn station_group_binding_by_id(
    connection: &Connection,
    id: &str,
) -> Result<StationGroupBinding, String> {
    connection
        .query_row(
            "SELECT * FROM station_group_bindings WHERE id = ?1",
            params![id],
            row_to_station_group_binding,
        )
        .optional()
        .map_err(|error| format!("读取分组绑定失败: {error}"))?
        .ok_or_else(|| "分组绑定不存在".to_string())
}

pub(crate) fn upsert_station_group_binding_in_connection(
    connection: &Connection,
    input: UpsertStationGroupBindingInput,
) -> Result<StationGroupBinding, String> {
    validate_station_exists(connection, &input.station_id)?;
    if let Some(station_key_id) = input.station_key_id.as_deref() {
        validate_station_key_exists(connection, station_key_id)?;
    }
    if input.group_name.trim().is_empty() {
        return Err("分组名称不能为空".to_string());
    }

    let binding_kind = validate_binding_kind(&input.binding_kind)?;
    let binding_status = validate_binding_status(&input.binding_status)?;
    let group_key_hash = validate_non_empty_hash(&input.group_key_hash)?;
    let now = now_string();
    let raw_json = input
        .raw_json_redacted
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("序列化 group raw 失败: {error}"))?;
    let inferred_group_category =
        normalize_group_category(input.inferred_group_category.as_deref());
    let group_category_override =
        normalize_group_category(input.group_category_override.as_deref());
    let existing_id = existing_group_binding_id(
        connection,
        &input.station_id,
        input.station_key_id.as_deref(),
        &binding_kind,
        &group_key_hash,
    )?;
    let previous_binding = existing_id
        .as_deref()
        .map(|id| station_group_binding_by_id(connection, id))
        .transpose()?;
    let was_new = existing_id.is_none();
    let id = existing_id.unwrap_or_else(|| generate_id("group_binding"));

    connection
        .execute(
            "INSERT INTO station_group_bindings (
                id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                group_key_hash, group_id_hash, group_name, binding_status,
                default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                inferred_group_category, group_category_override, rate_source, confidence,
                last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
             ON CONFLICT(id) DO UPDATE SET
                station_key_id = excluded.station_key_id,
                parent_group_binding_id = excluded.parent_group_binding_id,
                group_id_hash = excluded.group_id_hash,
                group_name = excluded.group_name,
                binding_status = CASE
                    WHEN station_group_bindings.binding_status = 'bound' AND excluded.binding_status NOT IN ('missing', 'disabled')
                    THEN station_group_bindings.binding_status
                    ELSE excluded.binding_status
                END,
                default_rate_multiplier = excluded.default_rate_multiplier,
                user_rate_multiplier = excluded.user_rate_multiplier,
                effective_rate_multiplier = excluded.effective_rate_multiplier,
                inferred_group_category = excluded.inferred_group_category,
                group_category_override = CASE
                    WHEN excluded.rate_source IN ('manual', 'remote_scan')
                    THEN excluded.group_category_override
                    ELSE COALESCE(station_group_bindings.group_category_override, excluded.group_category_override)
                END,
                rate_source = excluded.rate_source,
                confidence = excluded.confidence,
                last_seen_at = excluded.last_seen_at,
                last_checked_at = excluded.last_checked_at,
                last_rate_changed_at = excluded.last_rate_changed_at,
                raw_json_redacted = excluded.raw_json_redacted,
                updated_at = excluded.updated_at",
            params![
                id,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                binding_kind,
                normalize_optional_string(input.parent_group_binding_id),
                group_key_hash,
                normalize_optional_string(input.group_id_hash),
                input.group_name.trim(),
                binding_status,
                input.default_rate_multiplier,
                input.user_rate_multiplier,
                input.effective_rate_multiplier,
                inferred_group_category,
                group_category_override,
                normalize_optional_string(input.rate_source),
                clamp_confidence(input.confidence),
                normalize_optional_string(input.last_seen_at),
                now,
                None::<String>,
                raw_json,
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存分组绑定失败: {error}"))?;

    let saved = station_group_binding_by_id(connection, &id)?;
    disable_shadow_station_group_bindings(connection, &saved)?;
    emit_group_binding_change_events(connection, previous_binding.as_ref(), &saved);
    if was_new && saved.binding_kind == "station_group" && saved.binding_status == "available" {
        let event = crate::services::change_events::group_added_event(
            &saved.station_id,
            &saved.group_name,
            &saved.id,
            saved.default_rate_multiplier,
            saved.user_rate_multiplier,
            saved.effective_rate_multiplier,
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
    let _ = sync_group_added_event_rates_in_connection(connection, &saved);

    Ok(saved)
}

fn sync_group_added_event_rates_in_connection(
    connection: &Connection,
    binding: &StationGroupBinding,
) -> Result<(), String> {
    if binding.binding_kind != "station_group"
        || binding.binding_status != "available"
        || (binding.default_rate_multiplier.is_none()
            && binding.user_rate_multiplier.is_none()
            && binding.effective_rate_multiplier.is_none())
    {
        return Ok(());
    }

    let new_value_json = json!({
        "groupName": binding.group_name,
        "defaultRateMultiplier": binding.default_rate_multiplier,
        "userRateMultiplier": binding.user_rate_multiplier,
        "effectiveRateMultiplier": binding.effective_rate_multiplier
    })
    .to_string();
    let dedupe_key = crate::services::change_events::group_dedupe_key(
        &binding.station_id,
        "group_added",
        &binding.id,
    );

    connection
        .execute(
            "UPDATE change_events
                SET new_value_json = ?1
              WHERE event_type = 'group_added'
                AND dedupe_key = ?2",
            params![new_value_json, dedupe_key],
        )
        .map_err(|error| format!("同步新增分组事件倍率失败: {error}"))?;

    Ok(())
}

fn disable_shadow_station_group_bindings(
    connection: &Connection,
    saved: &StationGroupBinding,
) -> Result<(), String> {
    if saved.binding_kind != "station_group"
        || saved.binding_status != "available"
        || saved.rate_source.as_deref() == Some("remote_scan")
    {
        return Ok(());
    }

    connection
        .execute(
            "UPDATE station_group_bindings
                SET binding_status = 'disabled',
                    updated_at = ?1
              WHERE station_id = ?2
                AND binding_kind = 'station_group'
                AND id != ?3
                AND binding_status != 'disabled'
                AND rate_source = 'remote_scan'
                AND lower(trim(group_name)) = lower(trim(?4))",
            params![now_string(), saved.station_id, saved.id, saved.group_name],
        )
        .map_err(|error| format!("禁用重复分组绑定失败: {error}"))?;

    Ok(())
}

pub(crate) fn mark_missing_station_group_bindings_in_connection(
    connection: &Connection,
    station_id: &str,
    rate_sources: Vec<String>,
    present_group_key_hashes: Vec<String>,
) -> Result<(), String> {
    validate_station_exists(connection, station_id)?;
    let rate_sources = rate_sources
        .into_iter()
        .map(|source| source.trim().to_string())
        .filter(|source| !source.is_empty())
        .collect::<HashSet<_>>();
    if rate_sources.is_empty() {
        return Ok(());
    }
    let present_group_key_hashes = present_group_key_hashes.into_iter().collect::<HashSet<_>>();
    let bindings = list_station_group_bindings_from_connection(connection, station_id)?;

    for binding in bindings {
        if binding.binding_kind != "station_group"
            || binding.binding_status != "available"
            || !binding
                .rate_source
                .as_deref()
                .map(|source| rate_sources.contains(source))
                .unwrap_or(false)
            || present_group_key_hashes.contains(&binding.group_key_hash)
        {
            continue;
        }

        upsert_station_group_binding_in_connection(
            connection,
            UpsertStationGroupBindingInput {
                station_id: binding.station_id,
                station_key_id: binding.station_key_id,
                binding_kind: binding.binding_kind,
                parent_group_binding_id: binding.parent_group_binding_id,
                group_key_hash: binding.group_key_hash,
                group_id_hash: binding.group_id_hash,
                group_name: binding.group_name,
                binding_status: "missing".to_string(),
                default_rate_multiplier: binding.default_rate_multiplier,
                user_rate_multiplier: binding.user_rate_multiplier,
                effective_rate_multiplier: binding.effective_rate_multiplier,
                inferred_group_category: binding.inferred_group_category,
                group_category_override: binding.group_category_override,
                rate_source: binding.rate_source,
                confidence: binding.confidence,
                last_seen_at: binding.last_seen_at,
                raw_json_redacted: binding.raw_json_redacted,
            },
        )?;
    }

    Ok(())
}

fn emit_group_binding_change_events(
    connection: &Connection,
    previous: Option<&StationGroupBinding>,
    saved: &StationGroupBinding,
) {
    let previous_status = previous.map(|binding| binding.binding_status.as_str());
    if saved.binding_kind == "station_group"
        && saved.binding_status == "missing"
        && previous_status != Some("missing")
    {
        let event = crate::services::change_events::group_missing_event(
            &saved.station_id,
            &saved.group_name,
            &saved.id,
            saved.default_rate_multiplier,
            saved.user_rate_multiplier,
            saved.effective_rate_multiplier,
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }

    if saved.binding_kind != "key_binding" {
        return;
    }

    let Some(station_key_id) = saved.station_key_id.as_deref() else {
        return;
    };
    if saved.binding_status == "bound" && previous_status != Some("bound") {
        let event = crate::services::change_events::key_group_bound_event(
            &saved.station_id,
            station_key_id,
            &saved.id,
            &saved.group_name,
        );
        let _ = upsert_change_event_in_connection(connection, event);
    } else if saved.binding_status == "missing" && previous_status != Some("missing") {
        let event = crate::services::change_events::key_group_unresolved_event(
            &saved.station_id,
            station_key_id,
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
}

fn list_station_group_bindings_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<StationGroupBinding>, String> {
    validate_station_exists(connection, station_id)?;
    let mut statement = connection
        .prepare(
            "SELECT * FROM station_group_bindings
             WHERE station_id = ?1
             ORDER BY binding_kind ASC, binding_status ASC, group_name ASC",
        )
        .map_err(|error| format!("读取分组绑定失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_station_group_binding)
        .map_err(|error| format!("查询分组绑定失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析分组绑定失败: {error}"))?;
    Ok(rows)
}

fn list_group_rate_records_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<GroupRateRecord>, String> {
    validate_station_exists(connection, station_id)?;
    let mut statement = connection
        .prepare(
            "SELECT * FROM group_rate_records
             WHERE station_id = ?1
             ORDER BY checked_at DESC, created_at DESC",
        )
        .map_err(|error| format!("读取分组倍率历史失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_group_rate_record)
        .map_err(|error| format!("查询分组倍率历史失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析分组倍率历史失败: {error}"))?;
    Ok(rows)
}

pub(crate) fn insert_group_rate_record_if_changed_in_connection(
    connection: &Connection,
    input: InsertGroupRateRecordInput,
) -> Result<Option<GroupRateRecord>, String> {
    validate_station_exists(connection, &input.station_id)?;
    if let Some(station_key_id) = input.station_key_id.as_deref() {
        validate_station_key_exists(connection, station_key_id)?;
    }
    if let Some(group_binding_id) = input.group_binding_id.as_deref() {
        station_group_binding_by_id(connection, group_binding_id)?;
    }
    if input.group_name.trim().is_empty() {
        return Err("分组名称不能为空".to_string());
    }

    let binding_kind = validate_binding_kind(&input.binding_kind)?;
    let group_key_hash = validate_non_empty_hash(&input.group_key_hash)?;
    let previous = newest_group_rate_record(
        connection,
        &input.station_id,
        &binding_kind,
        &group_key_hash,
    )?;

    if previous
        .as_ref()
        .map(|record| group_rate_record_matches(record, &input))
        .unwrap_or(false)
    {
        return Ok(None);
    }

    let checked_at = normalize_optional_string(Some(input.checked_at)).unwrap_or_else(now_string);
    let id = generate_id("group_rate");
    let raw_json = input
        .raw_json_redacted
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("序列化倍率 raw 失败: {error}"))?;

    let inferred_group_category =
        normalize_group_category(input.inferred_group_category.as_deref());

    connection
        .execute(
            "INSERT INTO group_rate_records (
                id, station_id, station_key_id, group_binding_id, binding_kind,
                group_key_hash, group_name, default_rate_multiplier,
                user_rate_multiplier, effective_rate_multiplier, source, confidence,
                inferred_group_category, raw_json_redacted, checked_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                id,
                input.station_id,
                normalize_optional_string(input.station_key_id),
                normalize_optional_string(input.group_binding_id.clone()),
                binding_kind,
                group_key_hash,
                input.group_name.trim(),
                input.default_rate_multiplier,
                input.user_rate_multiplier,
                input.effective_rate_multiplier,
                input.source.trim(),
                clamp_confidence(input.confidence),
                inferred_group_category,
                raw_json,
                checked_at.clone(),
                now_string(),
            ],
        )
        .map_err(|error| format!("保存分组倍率历史失败: {error}"))?;

    if let Some(group_binding_id) = input.group_binding_id.as_deref() {
        connection
            .execute(
                "UPDATE station_group_bindings
                    SET last_rate_changed_at = ?2, updated_at = ?2
                  WHERE id = ?1",
                params![group_binding_id, checked_at],
            )
            .map_err(|error| format!("更新分组倍率变更时间失败: {error}"))?;
    }

    if let (Some(previous), Some(new_multiplier)) =
        (previous.as_ref(), input.effective_rate_multiplier)
    {
        if let Some(old_multiplier) = previous.effective_rate_multiplier {
            if let Some(event) = crate::services::change_events::rate_changed_event(
                &input.station_id,
                input.group_name.trim(),
                old_multiplier,
                new_multiplier,
            ) {
                let _ = upsert_change_event_in_connection(connection, event);
            }
        }
    }

    group_rate_record_by_id(connection, &id).map(Some)
}

fn newest_group_rate_record(
    connection: &Connection,
    station_id: &str,
    binding_kind: &str,
    group_key_hash: &str,
) -> Result<Option<GroupRateRecord>, String> {
    connection
        .query_row(
            "SELECT * FROM group_rate_records
              WHERE station_id = ?1 AND binding_kind = ?2 AND group_key_hash = ?3
              ORDER BY checked_at DESC, created_at DESC
              LIMIT 1",
            params![station_id, binding_kind, group_key_hash],
            row_to_group_rate_record,
        )
        .optional()
        .map_err(|error| format!("读取最新分组倍率历史失败: {error}"))
}

fn group_rate_record_by_id(connection: &Connection, id: &str) -> Result<GroupRateRecord, String> {
    connection
        .query_row(
            "SELECT * FROM group_rate_records WHERE id = ?1",
            params![id],
            row_to_group_rate_record,
        )
        .optional()
        .map_err(|error| format!("读取分组倍率历史失败: {error}"))?
        .ok_or_else(|| "分组倍率历史不存在".to_string())
}

fn group_rate_record_matches(record: &GroupRateRecord, input: &InsertGroupRateRecordInput) -> bool {
    record.group_name == input.group_name.trim()
        && optional_f64_eq(
            record.default_rate_multiplier,
            input.default_rate_multiplier,
        )
        && optional_f64_eq(record.user_rate_multiplier, input.user_rate_multiplier)
        && optional_f64_eq(
            record.effective_rate_multiplier,
            input.effective_rate_multiplier,
        )
        && record.inferred_group_category
            == normalize_group_category(input.inferred_group_category.as_deref())
}

fn optional_f64_eq(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => (left - right).abs() < f64::EPSILON,
        (None, None) => true,
        _ => false,
    }
}

fn list_collector_runs_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<CollectorRun>, String> {
    validate_station_exists(connection, station_id)?;
    let mut statement = connection
        .prepare(
            "SELECT * FROM collector_runs
             WHERE station_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|error| format!("读取采集运行记录失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_collector_run)
        .map_err(|error| format!("查询采集运行记录失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析采集运行记录失败: {error}"))?;
    Ok(rows)
}

fn validate_collector_task_type(value: &str) -> Result<String, String> {
    match value.trim() {
        "detect" | "balance" | "groups" | "models" | "full" => Ok(value.trim().to_string()),
        _ => Err("采集任务类型无效".to_string()),
    }
}

fn validate_collector_run_status(value: &str) -> Result<String, String> {
    match value.trim() {
        "running" | "success" | "partial" | "failed" | "manual_required" | "superseded" => {
            Ok(value.trim().to_string())
        }
        _ => Err("采集运行状态无效".to_string()),
    }
}

fn collector_run_by_id(connection: &Connection, id: &str) -> Result<CollectorRun, String> {
    connection
        .query_row(
            "SELECT * FROM collector_runs WHERE id = ?1",
            params![id],
            row_to_collector_run,
        )
        .optional()
        .map_err(|error| format!("读取采集运行记录失败: {error}"))?
        .ok_or_else(|| "采集运行记录不存在".to_string())
}

fn create_collector_run_in_connection(
    connection: &Connection,
    input: CreateCollectorRunInput,
) -> Result<CollectorRun, String> {
    let endpoint_revision = station_endpoint_revision(connection, &input.station_id)?;
    create_collector_run_in_connection_with_revision(connection, input, endpoint_revision)
}

pub(crate) fn create_collector_run_in_connection_with_revision(
    connection: &Connection,
    input: CreateCollectorRunInput,
    endpoint_revision: i64,
) -> Result<CollectorRun, String> {
    validate_station_exists(connection, &input.station_id)?;
    if let Some(parent_run_id) = input.parent_run_id.as_deref() {
        collector_run_by_id(connection, parent_run_id)?;
    }
    if input.adapter.trim().is_empty() {
        return Err("采集 adapter 不能为空".to_string());
    }
    let task_type = validate_collector_task_type(&input.task_type)?;
    let now = now_string();
    let id = generate_id("collector_run");
    connection
        .execute(
            "INSERT INTO collector_runs (
                id, station_id, endpoint_revision, parent_run_id, adapter, task_type, status,
                started_at, finished_at, duration_ms, endpoint_count, success_count,
                failure_count, manual_action_required, error_code, error_message,
                snapshot_id, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7, NULL, NULL, 0, 0, 0, 0, NULL, NULL, NULL, ?8)",
            params![
                id,
                input.station_id,
                endpoint_revision,
                normalize_optional_string(input.parent_run_id),
                input.adapter.trim(),
                task_type,
                now,
                now,
            ],
        )
        .map_err(|error| format!("创建采集运行记录失败: {error}"))?;
    collector_run_by_id(connection, &id)
}

pub(crate) fn finish_collector_run_in_connection(
    connection: &Connection,
    input: FinishCollectorRunInput,
) -> Result<CollectorRun, String> {
    let existing = collector_run_by_id(connection, &input.id)?;
    let status = validate_collector_run_status(&input.status)?;
    if status == "running" {
        return Err("完成采集运行记录时状态不能为 running".to_string());
    }
    if input.endpoint_count < 0 || input.success_count < 0 || input.failure_count < 0 {
        return Err("采集运行计数不能为负数".to_string());
    }
    let previous_run_status = latest_finished_collector_run_status(
        connection,
        &existing.station_id,
        &existing.task_type,
        Some(existing.id.as_str()),
    )?;
    let finished_at = now_string();
    let duration_ms = finished_at
        .parse::<i64>()
        .ok()
        .zip(existing.started_at.parse::<i64>().ok())
        .map(|(finished, started)| (finished - started).max(0));
    let error_message = redact_optional_text(input.error_message);

    connection
        .execute(
            "UPDATE collector_runs
             SET status = ?1,
                 finished_at = ?2,
                 duration_ms = ?3,
                 endpoint_count = ?4,
                 success_count = ?5,
                 failure_count = ?6,
                 manual_action_required = ?7,
                 error_code = ?8,
                 error_message = ?9,
                 snapshot_id = ?10
             WHERE id = ?11",
            params![
                status,
                finished_at,
                duration_ms,
                input.endpoint_count,
                input.success_count,
                input.failure_count,
                bool_to_i64(input.manual_action_required),
                normalize_optional_string(input.error_code),
                error_message,
                normalize_optional_string(input.snapshot_id),
                input.id,
            ],
        )
        .map_err(|error| format!("完成采集运行记录失败: {error}"))?;
    let saved = collector_run_by_id(connection, &input.id)?;
    update_station_collector_summary(connection, &saved.station_id, &saved.status, &finished_at)?;
    if matches!(saved.status.as_str(), "success" | "partial")
        && previous_run_status.as_deref() == Some("failed")
    {
        let failed_dedupe_key = crate::services::change_events::collector_dedupe_key(
            &saved.station_id,
            "collector_failed",
            &saved.task_type,
        );
        resolve_change_event_by_dedupe_key_in_connection(connection, &failed_dedupe_key)?;
        let event = crate::services::change_events::collector_recovered_event(
            &saved.station_id,
            &saved.id,
            &saved.task_type,
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
    Ok(saved)
}

fn update_station_collector_summary(
    connection: &Connection,
    station_id: &str,
    run_status: &str,
    checked_at: &str,
) -> Result<(), String> {
    let station_status = match run_status {
        "success" => Some("healthy"),
        "partial" | "manual_required" => Some("warning"),
        "failed" => Some("error"),
        _ => None,
    };
    let Some(station_status) = station_status else {
        return Ok(());
    };

    connection
        .execute(
            "UPDATE stations
                SET status = CASE WHEN status = 'disabled' THEN 'disabled' ELSE ?1 END,
                    last_checked_at = ?2,
                    last_pricing_fetched_at = CASE
                        WHEN ?3 IN ('success', 'partial') THEN ?2
                        ELSE last_pricing_fetched_at
                    END,
                    updated_at = ?2
              WHERE id = ?4",
            params![station_status, checked_at, run_status, station_id],
        )
        .map_err(|error| format!("同步站点采集状态失败: {error}"))?;

    Ok(())
}

fn latest_finished_collector_run_status(
    connection: &Connection,
    station_id: &str,
    task_type: &str,
    exclude_run_id: Option<&str>,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT status
               FROM collector_runs
              WHERE station_id = ?1
                AND task_type = ?2
                AND status != 'running'
                AND (?3 IS NULL OR id != ?3)
              ORDER BY COALESCE(finished_at, created_at) DESC, created_at DESC
              LIMIT 1",
            params![station_id, task_type, exclude_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取上一次采集状态失败: {error}"))
}

fn migrate_legacy_group_facts(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, group_name FROM station_keys
             WHERE group_name IS NOT NULL AND TRIM(group_name) != ''",
        )
        .map_err(|error| format!("读取旧 key 分组失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("查询旧 key 分组失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析旧 key 分组失败: {error}"))?;

    for (station_key_id, station_id, group_name) in rows {
        let group_key_hash = stable_group_key_hash(&station_id, "legacy", None, &group_name);
        let key_binding = upsert_station_group_binding_in_connection(
            connection,
            UpsertStationGroupBindingInput {
                station_id,
                station_key_id: Some(station_key_id.clone()),
                binding_kind: "key_binding".to_string(),
                parent_group_binding_id: None,
                group_key_hash,
                group_id_hash: None,
                group_name,
                binding_status: "manual_legacy".to_string(),
                default_rate_multiplier: None,
                user_rate_multiplier: None,
                effective_rate_multiplier: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                rate_source: Some("legacy_key_group".to_string()),
                confidence: 0.4,
                last_seen_at: None,
                raw_json_redacted: None,
            },
        )?;
        connection
            .execute(
                "UPDATE station_keys
                 SET group_binding_id = ?1,
                     group_id_hash = ?2,
                     updated_at = ?3
                 WHERE id = ?4",
                params![
                    key_binding.id,
                    key_binding.group_key_hash,
                    now_string(),
                    station_key_id
                ],
            )
            .map_err(|error| format!("回填 key 分组绑定失败: {error}"))?;
    }

    Ok(())
}

fn collector_snapshot_by_id(
    connection: &Connection,
    id: &str,
) -> Result<CollectorSnapshot, String> {
    connection
        .query_row(
            "SELECT id, station_id, endpoint_revision, source, status, fetched_at, summary_json,
                    normalized_json, raw_json_redacted, error_message, created_at
               FROM collector_snapshots
              WHERE id = ?1",
            params![id],
            row_to_collector_snapshot,
        )
        .optional()
        .map_err(|error| format!("读取采集快照失败: {error}"))?
        .ok_or_else(|| "采集快照不存在".to_string())
}

fn list_collector_snapshots_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<CollectorSnapshot>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, endpoint_revision, source, status, fetched_at, summary_json,
                    normalized_json, raw_json_redacted, error_message, created_at
               FROM collector_snapshots
              WHERE station_id = ?1
              ORDER BY created_at DESC",
        )
        .map_err(|error| format!("读取采集快照列表失败: {error}"))?;
    let rows = statement
        .query_map(params![station_id], row_to_collector_snapshot)
        .map_err(|error| format!("查询采集快照失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析采集快照失败: {error}"))?;
    Ok(rows)
}

fn latest_collector_snapshot_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Option<CollectorSnapshot>, String> {
    connection
        .query_row(
            "SELECT id, station_id, endpoint_revision, source, status, fetched_at, summary_json,
                    normalized_json, raw_json_redacted, error_message, created_at
               FROM collector_snapshots
              WHERE station_id = ?1
              ORDER BY created_at DESC
              LIMIT 1",
            params![station_id],
            row_to_collector_snapshot,
        )
        .optional()
        .map_err(|error| format!("读取最近采集快照失败: {error}"))
}

fn collector_task_type_from_snapshot_source(source: &str) -> String {
    source
        .rsplit_once('-')
        .map(|(_, task_type)| task_type.trim())
        .filter(|task_type| !task_type.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn parse_json_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| json!({ "parseError": true }))
}

fn models_from_snapshot_value(value: &Value) -> Vec<String> {
    value
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| {
                    model.as_str().map(ToString::to_string).or_else(|| {
                        model
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn rate_multipliers_from_snapshot_value(value: &Value) -> Vec<(String, f64)> {
    value
        .get("rateMultipliers")
        .and_then(Value::as_array)
        .map(|rates| {
            rates
                .iter()
                .filter_map(|rate| {
                    let group_name = rate
                        .get("groupName")
                        .or_else(|| rate.get("group"))
                        .or_else(|| rate.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("default")
                        .to_string();
                    let multiplier = rate
                        .get("multiplier")
                        .or_else(|| rate.get("rate"))
                        .or_else(|| rate.get("value"))
                        .and_then(Value::as_f64)?;
                    Some((group_name, multiplier))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_json_string_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn serialize_string_list(values: &[String]) -> Result<String, String> {
    let normalized = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    serde_json::to_string(&normalized).map_err(|error| format!("序列化字符串列表失败: {error}"))
}

fn summarize_capabilities(
    supports_chat: bool,
    supports_responses: bool,
    supports_embeddings: bool,
    supports_stream: bool,
    supports_tools: bool,
    supports_vision: bool,
    supports_reasoning: bool,
) -> Vec<String> {
    [
        (supports_chat, "Chat"),
        (supports_responses, "Responses"),
        (supports_embeddings, "Embeddings"),
        (supports_stream, "Stream"),
        (supports_tools, "Tools"),
        (supports_vision, "Vision"),
        (supports_reasoning, "Reasoning"),
    ]
    .into_iter()
    .filter_map(|(enabled, label)| enabled.then(|| label.to_string()))
    .collect()
}

fn summarize_model_scope(
    allowlist_count: usize,
    blocklist_count: usize,
    preferred_count: usize,
) -> String {
    let mut parts = Vec::new();
    if allowlist_count == 0 {
        parts.push("全部模型".to_string());
    } else {
        parts.push(format!("允许 {allowlist_count} 个模型"));
    }
    if blocklist_count > 0 {
        parts.push(format!("屏蔽 {blocklist_count} 个"));
    }
    if preferred_count > 0 {
        parts.push(format!("优先 {preferred_count} 个"));
    }
    parts.join("，")
}

fn success_rate(success_count: i64, failure_count: i64) -> Option<f64> {
    let total = success_count + failure_count;
    if total == 0 {
        return None;
    }
    Some(success_count as f64 / total as f64)
}

fn parse_upstream_api_format(value: String) -> UpstreamApiFormat {
    match value.as_str() {
        "openai_chat_completions" => UpstreamApiFormat::OpenAiChatCompletions,
        "openai_responses" => UpstreamApiFormat::OpenAiResponses,
        "custom_openai_compatible" => UpstreamApiFormat::CustomOpenAiCompatible,
        _ => UpstreamApiFormat::Auto,
    }
}

fn serialize_routing_group_filter_setting(filter: &RoutingGroupFilter) -> Result<String, String> {
    match serde_json::to_value(filter)
        .map_err(|error| format!("serialize routing group filter failed: {error}"))?
    {
        Value::String(value) => Ok(value),
        value => serde_json::to_string(&value)
            .map_err(|error| format!("serialize routing group filter failed: {error}")),
    }
}

fn parse_routing_group_filter_setting(value: &str) -> Result<RoutingGroupFilter, String> {
    if value.trim().is_empty() {
        return Ok(RoutingGroupFilter::AllGroups);
    }
    serde_json::from_str::<RoutingGroupFilter>(value)
        .or_else(|_| serde_json::from_value::<RoutingGroupFilter>(Value::String(value.to_string())))
        .map_err(|error| format!("parse routing group filter failed: {error}"))
}

fn parse_optional_f64_setting(value: &str, key: &str) -> Result<Option<f64>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("setting {key} has invalid number format"))?;
    if !parsed.is_finite() {
        return Err(format!("setting {key} must be finite"));
    }
    Ok(Some(parsed))
}

fn parse_scheduler_advanced_settings(value: &str) -> Result<SchedulerAdvancedSettings, String> {
    if value.trim().is_empty() {
        return Ok(SchedulerAdvancedSettings::default());
    }
    let settings: SchedulerAdvancedSettings = serde_json::from_str(value)
        .map_err(|error| format!("parse scheduler advanced settings failed: {error}"))?;
    settings
        .validate()
        .map_err(|error| format!("scheduler advanced settings invalid: {error:?}"))?;
    Ok(settings)
}

fn settings_from_connection(
    connection: &Connection,
    data_dir: &str,
    pending_data_dir: Option<String>,
) -> Result<AppSettings, String> {
    let local_key = read_setting(connection, "local_key")?;
    let data_dir_change_requires_restart = pending_data_dir
        .as_ref()
        .map(|pending| pending != data_dir)
        .unwrap_or(false);

    Ok(AppSettings {
        local_proxy_port: parse_setting(connection, "local_proxy_port")?,
        local_key_masked: mask_secret(&local_key),
        default_routing_strategy: read_setting(connection, "default_routing_strategy")?,
        collector_proxy_mode: normalize_proxy_mode(
            &read_setting_or_default(connection, "collector_proxy_mode", "direct")?,
            false,
        ),
        collector_proxy_url: normalize_proxy_url(Some(read_setting_or_default(
            connection,
            "collector_proxy_url",
            "",
        )?)),
        max_rate_multiplier: parse_optional_f64_setting(
            &read_setting_or_default(connection, "max_rate_multiplier", "")?,
            "max_rate_multiplier",
        )?,
        default_routing_group_filter: parse_routing_group_filter_setting(
            &read_setting_or_default(connection, "default_routing_group_filter", "all_groups")?,
        )?,
        scheduler_advanced_settings: parse_scheduler_advanced_settings(&read_setting_or_default(
            connection,
            "scheduler_advanced_settings_json",
            "",
        )?)?,
        low_balance_threshold_cny: parse_setting(connection, "low_balance_threshold_cny")?,
        collector_interval_minutes: parse_setting(connection, "collector_interval_minutes")?,
        balance_interval_minutes: parse_setting_or_default(
            connection,
            "balance_interval_minutes",
            "5",
        )?,
        group_rate_interval_minutes: parse_setting_or_default(
            connection,
            "group_rate_interval_minutes",
            "20",
        )?,
        model_list_interval_minutes: parse_setting_or_default(
            connection,
            "model_list_interval_minutes",
            "60",
        )?,
        pricing_refresh_interval_minutes: parse_setting_or_default(
            connection,
            "pricing_refresh_interval_minutes",
            "60",
        )?,
        collector_timeout_seconds: parse_setting_or_default(
            connection,
            "collector_timeout_seconds",
            "15",
        )?,
        collector_max_concurrency: parse_setting_or_default(
            connection,
            "collector_max_concurrency",
            "3",
        )?,
        allow_depleted_fallback: parse_setting_or_default(
            connection,
            "allow_depleted_fallback",
            "false",
        )?,
        developer_mode_enabled: read_setting_or_default(
            connection,
            "developer_mode_enabled",
            "false",
        )?
        .parse()
        .map_err(|_| "设置项 developer_mode_enabled 格式无效".to_string())?,
        data_dir: data_dir.to_string(),
        pending_data_dir,
        data_dir_change_requires_restart,
    })
}

fn read_setting(connection: &Connection, key: &str) -> Result<String, String> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取设置 {key} 失败: {error}"))?
        .ok_or_else(|| format!("缺少设置项: {key}"))
}

fn read_setting_or_default(
    connection: &Connection,
    key: &str,
    default_value: &str,
) -> Result<String, String> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取设置 {key} 失败: {error}"))
        .map(|value| value.unwrap_or_else(|| default_value.to_string()))
}

fn parse_setting<T>(connection: &Connection, key: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    read_setting(connection, key)?
        .parse()
        .map_err(|_| format!("设置项 {key} 格式无效"))
}

fn parse_setting_or_default<T>(
    connection: &Connection,
    key: &str,
    default_value: &str,
) -> Result<T, String>
where
    T: std::str::FromStr,
{
    read_setting_or_default(connection, key, default_value)?
        .parse()
        .map_err(|_| format!("设置项 {key} 格式无效"))
}

fn upsert_setting(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_string()],
        )
        .map_err(|error| format!("保存设置 {key} 失败: {error}"))?;

    Ok(())
}

fn validate_station_fields(
    name: &str,
    station_type: &str,
    website_url: &str,
    credit_per_cny: f64,
    collection_interval_minutes: u16,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("站点名称不能为空".to_string());
    }
    if !matches!(
        station_type,
        "sub2api" | "newapi" | "openai-compatible" | "custom"
    ) {
        return Err("站点类型无效".to_string());
    }
    if website_url.trim().is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    if credit_per_cny <= 0.0 {
        return Err("充值兑换比例必须大于 0".to_string());
    }
    if collection_interval_minutes == 0 {
        return Err("Collection interval must be greater than 0 minutes".to_string());
    }

    Ok(())
}

fn validate_proxy_config(
    mode: String,
    url: Option<String>,
    allow_inherit: bool,
) -> Result<String, String> {
    let normalized_mode = normalize_proxy_mode(&mode, allow_inherit);
    let allowed = if allow_inherit {
        matches!(
            normalized_mode.as_str(),
            "inherit" | "direct" | "system" | "manual"
        )
    } else {
        matches!(normalized_mode.as_str(), "direct" | "system" | "manual")
    };
    if !allowed || normalized_mode != mode.trim() {
        return Err("Proxy mode is invalid".to_string());
    }
    if normalized_mode == "manual" && normalize_proxy_url(url).is_none() {
        return Err("Manual proxy URL is required".to_string());
    }
    Ok(normalized_mode)
}

fn next_station_priority(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(priority), -1) + 1 FROM stations",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("计算站点排序失败: {error}"))
}

fn normalize_station_priorities(connection: &Connection) -> Result<(), String> {
    let ids = {
        let mut statement = connection
            .prepare("SELECT id FROM stations ORDER BY priority ASC, created_at ASC")
            .map_err(|error| format!("读取排序失败: {error}"))?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("查询排序失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("解析排序失败: {error}"))?;

        ids
    };

    for (index, id) in ids.iter().enumerate() {
        connection
            .execute(
                "UPDATE stations SET priority = ?1, updated_at = ?2 WHERE id = ?3",
                params![index as i64, now_string(), id],
            )
            .map_err(|error| format!("整理站点排序失败: {error}"))?;
    }

    Ok(())
}

fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "未设置".to_string();
    }
    if trimmed.len() <= 8 {
        return "****".to_string();
    }

    let prefix: String = trimmed.chars().take(4).collect();
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    format!("{prefix}****{suffix}")
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_required_string(value: String, fallback: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized.to_string()
    }
}

fn redact_optional_text(value: Option<String>) -> Option<String> {
    normalize_optional_string(value).map(|item| redact_sensitive_text(&item))
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

fn generate_id(prefix: &str) -> String {
    let sequence = NEXT_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{sequence}", now_millis())
}

fn now_string() -> String {
    now_millis().to_string()
}

pub fn now_millis_for_services() -> u128 {
    now_millis()
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::channel_monitors::{
        CreateChannelMonitorInput, CreateChannelMonitorRunInput, CreateChannelMonitorTemplateInput,
        UpdateChannelMonitorTemplateInput,
    };
    use crate::models::group_facts::{
        UpdateStationKeyGroupBindingInput, BINDING_KIND_KEY_BINDING, BINDING_KIND_STATION_GROUP,
        BINDING_STATUS_AVAILABLE, BINDING_STATUS_MISSING,
    };
    use crate::models::pricing::{
        UpsertBalanceSnapshotInput, UpsertModelBasePriceInput, UpsertPricingRuleInput,
    };
    use crate::models::proxy::ProxyLifecycle;
    use crate::models::routing::{PricingGroupType, RouteEndpointKind, RoutingGroupFilter};

    fn create_legacy_station_endpoint_schema(connection: &Connection) {
        connection
            .execute_batch(
                r#"
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    base_url TEXT NOT NULL,
                    upstream_api_base_path TEXT NOT NULL DEFAULT '/v1'
                );
                CREATE TABLE collector_snapshots (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    summary_json TEXT NOT NULL,
                    normalized_json TEXT NOT NULL,
                    raw_json_redacted TEXT
                );
                CREATE TABLE collector_runs (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL
                );
                CREATE TABLE station_endpoint_health (
                    station_id TEXT PRIMARY KEY
                );
                CREATE TABLE station_key_health (
                    station_key_id TEXT PRIMARY KEY
                );
                "#,
            )
            .expect("legacy station endpoint schema");
    }

    fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("table info");
        statement
            .query_map([], |row| row.get(1))
            .expect("query columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns")
    }

    #[test]
    fn station_endpoint_migration_separates_legacy_root_and_versioned_urls() {
        let connection = Connection::open_in_memory().expect("connection");
        create_legacy_station_endpoint_schema(&connection);
        connection
            .execute_batch(
                r#"
                INSERT INTO stations (id, base_url) VALUES
                    ('root', 'https://root.example'),
                    ('v1', 'https://v1.example/v1'),
                    ('ark', 'https://ark.example/api/v3');
                INSERT INTO collector_snapshots (
                    id, station_id, summary_json, normalized_json, raw_json_redacted
                ) VALUES ('snapshot', 'root', '{"legacy":true}', '{"legacy":true}', '{"legacy":true}');
                INSERT INTO collector_runs (id, station_id) VALUES ('run', 'root');
                INSERT INTO station_endpoint_health (station_id) VALUES ('root');
                INSERT INTO station_key_health (station_key_id) VALUES ('key');
                "#,
            )
            .expect("legacy endpoint rows");

        migrate_station_endpoint_urls(&connection).expect("migrate endpoints");

        let rows = connection
            .prepare(
                "SELECT id, website_url, api_base_url, endpoint_revision
                   FROM stations
                  ORDER BY id",
            )
            .expect("station endpoints")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .expect("query endpoints")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect endpoints");
        assert_eq!(
            rows,
            vec![
                (
                    "ark".to_string(),
                    "https://ark.example/api/v3".to_string(),
                    "https://ark.example/api/v3".to_string(),
                    1,
                ),
                (
                    "root".to_string(),
                    "https://root.example".to_string(),
                    "https://root.example/v1".to_string(),
                    1,
                ),
                (
                    "v1".to_string(),
                    "https://v1.example".to_string(),
                    "https://v1.example/v1".to_string(),
                    1,
                ),
            ]
        );

        let station_columns = table_columns(&connection, "stations");
        assert!(!station_columns.iter().any(|column| column == "base_url"));
        assert!(!station_columns
            .iter()
            .any(|column| column == "upstream_api_base_path"));
        for table in [
            "collector_snapshots",
            "collector_runs",
            "station_endpoint_health",
            "station_key_health",
        ] {
            assert!(table_columns(&connection, table)
                .iter()
                .any(|column| column == "endpoint_revision"));
            let revision: i64 = connection
                .query_row(
                    &format!("SELECT endpoint_revision FROM {table} LIMIT 1"),
                    [],
                    |row| row.get(0),
                )
                .expect("derived endpoint revision");
            assert_eq!(revision, 1, "{table}");
        }
        let historical_json: (String, String, String) = connection
            .query_row(
                "SELECT summary_json, normalized_json, raw_json_redacted
                   FROM collector_snapshots WHERE id = 'snapshot'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("historical snapshot");
        assert_eq!(
            historical_json,
            (
                "{\"legacy\":true}".to_string(),
                "{\"legacy\":true}".to_string(),
                "{\"legacy\":true}".to_string(),
            )
        );

        migrate_station_endpoint_urls(&connection).expect("repeat endpoint migration");
        assert_eq!(
            table_columns(&connection, "stations")
                .iter()
                .filter(|column| column.as_str() == "api_base_url")
                .count(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM stations", [], |row| row
                    .get::<_, i64>(0))
                .expect("station count"),
            3
        );
    }

    #[test]
    fn station_endpoint_migration_accepts_matching_transitional_api_url() {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                r#"
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    base_url TEXT NOT NULL,
                    api_base_url TEXT NOT NULL,
                    upstream_api_base_path TEXT NOT NULL DEFAULT '/v1'
                );
                INSERT INTO stations (id, base_url, api_base_url)
                VALUES ('station', 'https://relay.example/', 'https://relay.example/v1/');
                "#,
            )
            .expect("transitional schema");

        migrate_station_endpoint_urls(&connection).expect("migrate transitional endpoints");

        let endpoints: (String, String, i64) = connection
            .query_row(
                "SELECT website_url, api_base_url, endpoint_revision FROM stations",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("station endpoints");
        assert_eq!(
            endpoints,
            (
                "https://relay.example".to_string(),
                "https://relay.example/v1".to_string(),
                1,
            )
        );
    }

    #[test]
    fn station_endpoint_migration_fails_closed_on_conflicting_states() {
        let conflicting_values = Connection::open_in_memory().expect("connection");
        conflicting_values
            .execute_batch(
                r#"
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    base_url TEXT NOT NULL,
                    api_base_url TEXT NOT NULL
                );
                INSERT INTO stations (id, base_url, api_base_url)
                VALUES ('station', 'https://relay.example', 'https://other.example/v1');
                "#,
            )
            .expect("conflicting values schema");
        let error = migrate_station_endpoint_urls(&conflicting_values)
            .expect_err("conflicting endpoint values must fail");
        assert!(error.to_string().contains("conflict"));
        assert!(table_columns(&conflicting_values, "stations")
            .iter()
            .any(|column| column == "base_url"));

        let conflicting_columns = Connection::open_in_memory().expect("connection");
        conflicting_columns
            .execute_batch(
                r#"
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    base_url TEXT NOT NULL,
                    website_url TEXT NOT NULL,
                    api_base_url TEXT NOT NULL
                );
                "#,
            )
            .expect("conflicting columns schema");
        let error = migrate_station_endpoint_urls(&conflicting_columns)
            .expect_err("ambiguous endpoint columns must fail");
        assert!(error.to_string().contains("conflict"));
    }

    #[test]
    fn station_create_round_trips_both_urls() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "separate endpoint roles".to_string(),
                station_type: "openai-compatible".to_string(),
                website_url: " https://console.example/ ".to_string(),
                api_base_url: " https://gateway.example/v1/ ".to_string(),
                api_key: "sk-test".to_string(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");

        assert_eq!(station.website_url, "https://console.example");
        assert_eq!(station.api_base_url, "https://gateway.example/v1");
        assert_eq!(station.endpoint_revision, 1);
        let loaded = database
            .station_for_collector(&station.id)
            .expect("stored station");
        assert_eq!(loaded.website_url, station.website_url);
        assert_eq!(loaded.api_base_url, station.api_base_url);
        assert_eq!(loaded.endpoint_revision, 1);
    }

    fn test_station(database: &AppDatabase, name: &str) -> Station {
        database
            .create_station(CreateStationInput {
                name: name.to_string(),
                station_type: "openai-compatible".to_string(),
                website_url: "https://example.test".to_string(),
                api_base_url: "https://example.test/v1".to_string(),
                api_key: "sk-test-routing".to_string(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station")
    }

    fn change_test_station_endpoint(database: &AppDatabase, station: &Station) -> Station {
        database
            .update_station(UpdateStationInput {
                id: station.id.clone(),
                name: station.name.clone(),
                station_type: station.station_type.clone(),
                website_url: "https://replacement.example.test".to_string(),
                api_base_url: "https://replacement.example.test/v1".to_string(),
                api_key: None,
                collector_proxy_mode: station.collector_proxy_mode.clone(),
                collector_proxy_url: station.collector_proxy_url.clone(),
                enabled: station.enabled,
                credit_per_cny: station.credit_per_cny,
                low_balance_threshold_cny: station.low_balance_threshold_cny,
                collection_interval_minutes: station.collection_interval_minutes,
                note: station.note.clone(),
            })
            .expect("change station endpoint")
    }

    fn update_test_station_urls(
        database: &AppDatabase,
        station: &Station,
        website_url: String,
        api_base_url: String,
        enabled: bool,
    ) -> Station {
        database
            .update_station(UpdateStationInput {
                id: station.id.clone(),
                name: station.name.clone(),
                station_type: station.station_type.clone(),
                website_url,
                api_base_url,
                api_key: None,
                collector_proxy_mode: station.collector_proxy_mode.clone(),
                collector_proxy_url: station.collector_proxy_url.clone(),
                enabled,
                credit_per_cny: station.credit_per_cny,
                low_balance_threshold_cny: station.low_balance_threshold_cny,
                collection_interval_minutes: station.collection_interval_minutes,
                note: station.note.clone(),
            })
            .expect("update station URLs")
    }

    fn update_test_station_name(database: &AppDatabase, station: &Station, name: &str) -> Station {
        database
            .update_station(UpdateStationInput {
                id: station.id.clone(),
                name: name.to_string(),
                station_type: station.station_type.clone(),
                website_url: station.website_url.clone(),
                api_base_url: station.api_base_url.clone(),
                api_key: None,
                collector_proxy_mode: station.collector_proxy_mode.clone(),
                collector_proxy_url: station.collector_proxy_url.clone(),
                enabled: station.enabled,
                credit_per_cny: station.credit_per_cny,
                low_balance_threshold_cny: station.low_balance_threshold_cny,
                collection_interval_minutes: station.collection_interval_minutes,
                note: station.note.clone(),
            })
            .expect("rename station")
    }

    fn seed_station_and_key_health(database: &AppDatabase, station_id: &str) {
        let key = database
            .list_station_keys(station_id.to_string())
            .expect("station keys")
            .remove(0);
        database
            .upsert_station_endpoint_health(station_id, "success", Some(38), "1000", None)
            .expect("endpoint health");
        database
            .record_station_key_success(&key.id, 45, "1000")
            .expect("key health");
        let connection = database.connection().expect("connection");
        connection
            .execute(
                "UPDATE stations
                    SET status = 'healthy',
                        last_checked_at = '1000',
                        last_pricing_fetched_at = '1000'
                  WHERE id = ?1",
                params![station_id],
            )
            .expect("seed station collection state");
    }

    fn assert_station_key_health_cleared(database: &AppDatabase, station_id: &str) {
        let key = database
            .list_station_keys(station_id.to_string())
            .expect("station keys")
            .remove(0);
        let health = database.get_station_key_health(key.id).expect("key health");
        assert_eq!(health.last_success_at, None);
        assert_eq!(health.last_failure_at, None);
        assert_eq!(health.success_count, 0);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.avg_latency_ms, None);
        assert_eq!(health.last_error_summary, None);
        assert_eq!(health.cooldown_until, None);
    }

    fn assert_station_health_rows_deleted(database: &AppDatabase, station_id: &str) {
        let key = database
            .list_station_keys(station_id.to_string())
            .expect("station keys")
            .remove(0);
        let connection = database.connection().expect("connection");
        let endpoint_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM station_endpoint_health WHERE station_id = ?1",
                params![station_id],
                |row| row.get(0),
            )
            .expect("count endpoint health rows");
        let key_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM station_key_health WHERE station_key_id = ?1",
                params![key.id],
                |row| row.get(0),
            )
            .expect("count key health rows");
        assert_eq!(endpoint_rows, 0);
        assert_eq!(key_rows, 0);
    }

    fn station_with_saved_credentials(database: &AppDatabase) -> Station {
        let station = test_station(database, "saved-login-material");
        let data_key = [17_u8; 32];
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.com".to_string()),
                    login_password: Some("saved-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("saved password");
        database
            .persist_station_session_with_data_key(
                PersistStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("saved-access-token".to_string()),
                    refresh_token: None,
                    cookie: Some("session=saved-cookie".to_string()),
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: "password_login".to_string(),
                },
                &data_key,
            )
            .expect("saved session");
        station
    }

    #[test]
    fn api_origin_change_disables_station_increments_revision_and_clears_health() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "origin-change");
        seed_station_and_key_health(&database, &station.id);

        let updated = update_test_station_urls(
            &database,
            &station,
            station.website_url.clone(),
            "https://new-api.example/v1".to_string(),
            true,
        );

        assert!(!updated.enabled);
        assert_eq!(updated.endpoint_revision, station.endpoint_revision + 1);
        assert_eq!(updated.status, "disabled");
        assert_eq!(updated.last_checked_at, None);
        assert_eq!(updated.last_pricing_fetched_at, None);
        assert_eq!(
            database
                .get_station_endpoint_health(updated.id.clone())
                .unwrap()
                .status,
            "unchecked"
        );
        assert_station_key_health_cleared(&database, &updated.id);
    }

    #[test]
    fn api_namespace_change_clears_health_without_forcing_disable() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "api-path-change");
        seed_station_and_key_health(&database, &station.id);

        let updated = update_test_station_urls(
            &database,
            &station,
            station.website_url.clone(),
            "https://example.test/api/v3".to_string(),
            true,
        );

        assert!(updated.enabled);
        assert_eq!(updated.endpoint_revision, station.endpoint_revision + 1);
        assert_eq!(updated.status, "unchecked");
        assert_eq!(updated.last_checked_at, None);
        assert_eq!(updated.last_pricing_fetched_at, None);
        assert_station_health_rows_deleted(&database, &updated.id);
    }

    #[test]
    fn website_origin_change_clears_secret_login_material_but_keeps_username() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = station_with_saved_credentials(&database);

        let updated = update_test_station_urls(
            &database,
            &station,
            "https://new-console.example".to_string(),
            station.api_base_url.clone(),
            true,
        );

        let credentials = database
            .get_station_credentials(updated.id)
            .expect("credentials");
        assert_eq!(
            credentials.login_username.as_deref(),
            Some("user@example.com")
        );
        assert!(!credentials.password_present);
        assert!(!credentials.access_token_present);
        assert!(!credentials.refresh_token_present);
        assert!(!credentials.cookie_present);
        assert_eq!(credentials.newapi_user_id, None);
        assert_eq!(credentials.session_status, "none");
        assert_eq!(updated.endpoint_revision, station.endpoint_revision + 1);
    }

    #[test]
    fn unrelated_station_edit_does_not_increment_endpoint_revision() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "rename-only");

        let updated = update_test_station_name(&database, &station, "Renamed");

        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.endpoint_revision, station.endpoint_revision);
    }

    fn set_station_type_for_test(database: &AppDatabase, station_id: &str, station_type: &str) {
        let connection = database.connection().expect("connection");
        connection
            .execute(
                "UPDATE stations SET station_type = ?2 WHERE id = ?1",
                params![station_id, station_type],
            )
            .expect("set station type");
    }

    #[test]
    fn known_placeholder_local_key_is_rotated_once() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let first = database
            .ensure_secure_local_access_key()
            .expect("secure local key");
        let second = database
            .ensure_secure_local_access_key()
            .expect("stable local key");

        assert_ne!(first, "sk-local-pool-change-me");
        assert!(first.starts_with("sk-local-"));
        assert!(first.len() >= 50);
        assert_eq!(second, first);
    }

    #[test]
    fn rich_route_candidates_preserve_unschedulable_state() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "unschedulable-rich-candidate");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "paused key".to_string(),
                api_key: "sk-paused".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: Some(false),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");

        let candidates = database
            .proxy_rich_route_candidates()
            .expect("rich candidates");
        let candidate = candidates
            .iter()
            .find(|item| item.candidate.station_key_id == key.id)
            .expect("candidate remains explainable");

        assert!(!candidate.candidate.schedulable);
    }

    fn finish_test_collector_run(
        database: &AppDatabase,
        station_id: &str,
        task_type: &str,
        status: &str,
    ) {
        let run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station_id.to_string(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: task_type.to_string(),
            })
            .expect("create collector run");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: run.id,
                status: status.to_string(),
                endpoint_count: 1,
                success_count: i64::from(status == "success"),
                failure_count: i64::from(status == "failed"),
                manual_action_required: false,
                error_code: None,
                error_message: None,
                snapshot_id: None,
            })
            .expect("finish collector run");
    }

    fn test_channel_monitor(database: &AppDatabase, name: &str) -> ChannelMonitor {
        let station = test_station(database, name);
        database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: format!("{name} monitor"),
                target_type: "station".to_string(),
                station_id: station.id,
                station_key_id: None,
                template_id: "builtin-openai-chat-default".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 15,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("monitor")
    }

    fn assert_option_f64_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("expected value");
        assert!(
            (actual - expected).abs() < 0.000000001,
            "expected {expected}, got {actual}"
        );
    }

    fn candidate_key_ids(candidates: &[SchedulerCandidate]) -> Vec<String> {
        candidates
            .iter()
            .map(|candidate| candidate.station_key_id.clone())
            .collect()
    }

    fn scheduler_group_candidate(
        database: &AppDatabase,
        station_id: &str,
        key_name: &str,
        group_name: &str,
        group_id_hash: &str,
        priority: i64,
    ) -> StationKey {
        scheduler_group_candidate_with_category(
            database,
            station_id,
            key_name,
            group_name,
            group_id_hash,
            priority,
            None,
            None,
        )
    }

    fn scheduler_group_candidate_with_category(
        database: &AppDatabase,
        station_id: &str,
        key_name: &str,
        group_name: &str,
        group_id_hash: &str,
        priority: i64,
        inferred_group_category: Option<&str>,
        group_category_override: Option<&str>,
    ) -> StationKey {
        let binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station_id.to_string(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: format!("station:{group_id_hash}"),
                group_id_hash: Some(group_id_hash.to_string()),
                group_name: group_name.to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("group_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: inferred_group_category.map(str::to_string),
                group_category_override: group_category_override.map(str::to_string),
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("group binding");

        database
            .create_station_key(CreateStationKeyInput {
                station_id: station_id.to_string(),
                name: key_name.to_string(),
                api_key: format!("sk-{group_id_hash}"),
                enabled: true,
                priority: Some(priority),
                max_concurrency: None,
                load_factor: None,
                schedulable: Some(true),
                group_name: Some(group_name.to_string()),
                tier_label: None,
                group_binding_id: Some(binding.id),
                group_id_hash: Some(group_id_hash.to_string()),
                rate_multiplier: Some(1.0),
                manual_rate_multiplier: None,
                rate_source: Some("group_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("scheduler group candidate key")
    }

    #[test]
    fn load_multiplier_source_facts_preserves_binding_confidence_for_collected_rate() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "multiplier-confidence");
        let binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:pro".to_string(),
                group_id_hash: Some("hash-pro".to_string()),
                group_name: "pro".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.25),
                effective_rate_multiplier: Some(1.25),
                rate_source: Some("group_rates".to_string()),
                confidence: 0.86,
                inferred_group_category: None,
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("binding");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "collected key".to_string(),
                api_key: "sk-collected".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: Some("pro".to_string()),
                tier_label: None,
                group_binding_id: Some(binding.id.clone()),
                group_id_hash: Some("hash-pro".to_string()),
                rate_multiplier: Some(1.25),
                manual_rate_multiplier: None,
                rate_source: Some("group_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("key");

        let facts = database
            .load_multiplier_source_facts(&key.id)
            .expect("multiplier source facts");

        assert_eq!(facts.group_binding_id.as_deref(), Some(binding.id.as_str()));
        assert_option_f64_close(facts.collected_rate_confidence, 0.86);
        crate::services::proxy::scheduler::multiplier::resolve_effective_multiplier(
            facts,
            0,
            0.8,
            20 * 60 * 1000,
        )
        .expect("binding confidence should satisfy default multiplier threshold");
    }

    #[test]
    fn load_scheduler_candidates_applies_specific_group_filter_without_fallback() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-candidates-filter");
        let gpt_binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:gpt".to_string(),
                group_id_hash: Some("hash-gpt".to_string()),
                group_name: "gpt".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("group_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: None,
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("gpt binding");
        let claude_binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:claude".to_string(),
                group_id_hash: Some("hash-claude".to_string()),
                group_name: "claude".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.1),
                user_rate_multiplier: Some(0.1),
                effective_rate_multiplier: Some(0.1),
                rate_source: Some("group_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: None,
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("claude binding");
        let gpt_key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "gpt key".to_string(),
                api_key: "sk-gpt".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: Some(7),
                load_factor: Some(9),
                schedulable: Some(true),
                group_name: Some("gpt".to_string()),
                tier_label: None,
                group_binding_id: Some(gpt_binding.id.clone()),
                group_id_hash: Some("hash-gpt".to_string()),
                rate_multiplier: Some(1.0),
                manual_rate_multiplier: None,
                rate_source: Some("group_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("gpt key");
        database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "claude key".to_string(),
                api_key: "sk-claude".to_string(),
                enabled: true,
                priority: Some(1),
                max_concurrency: None,
                load_factor: None,
                schedulable: Some(true),
                group_name: Some("claude".to_string()),
                tier_label: None,
                group_binding_id: Some(claude_binding.id),
                group_id_hash: Some("hash-claude".to_string()),
                rate_multiplier: Some(0.1),
                manual_rate_multiplier: None,
                rate_source: Some("group_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("claude key");

        let candidates = database
            .load_scheduler_candidates(&RoutingGroupFilter::GroupBindingId(gpt_binding.id), 0)
            .expect("scheduler candidates");

        assert_eq!(candidate_key_ids(&candidates), vec![gpt_key.id]);
        assert_eq!(candidates[0].max_concurrency, 7);
        assert_eq!(candidates[0].load_factor, Some(9));

        let missing = database
            .load_scheduler_candidates(&RoutingGroupFilter::GroupIdHash("missing".to_string()), 0)
            .expect("missing group filter candidates");
        assert!(
            missing.is_empty(),
            "specific group filters must not fall back to all groups"
        );
    }

    #[test]
    fn load_scheduler_candidates_group_type_filter_uses_canonical_aliases_without_fallback() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-candidates-group-type-aliases");

        let image_generation_key = scheduler_group_candidate(
            &database,
            &station.id,
            "image-generation key",
            "image-generation",
            "hash-image-generation",
            0,
        );
        let image_generation_space_key = scheduler_group_candidate(
            &database,
            &station.id,
            "image generation key",
            "image generation",
            "hash-image-generation-space",
            1,
        );
        let image_generation_slug_key = scheduler_group_candidate(
            &database,
            &station.id,
            "image_generation key",
            "image_generation",
            "hash-image-generation-slug",
            2,
        );
        let image_generation_chinese_key = scheduler_group_candidate(
            &database,
            &station.id,
            "gpt image key",
            "GPT画图分组",
            "hash-gpt-image",
            3,
        );
        scheduler_group_candidate(&database, &station.id, "gpt key", "gpt", "hash-gpt", 4);

        let candidates = database
            .load_scheduler_candidates(
                &RoutingGroupFilter::GroupType(PricingGroupType::ImageGeneration),
                0,
            )
            .expect("scheduler candidates");

        assert_eq!(
            candidate_key_ids(&candidates),
            vec![
                image_generation_key.id,
                image_generation_space_key.id,
                image_generation_slug_key.id,
                image_generation_chinese_key.id,
            ]
        );
        assert!(candidates
            .iter()
            .all(|candidate| candidate.group_type == Some(PricingGroupType::ImageGeneration)));

        let missing = database
            .load_scheduler_candidates(&RoutingGroupFilter::GroupType(PricingGroupType::Grok), 0)
            .expect("missing group type candidates");
        assert!(
            missing.is_empty(),
            "group type filters must not fall back to unrelated groups"
        );
    }

    #[test]
    fn load_scheduler_candidates_group_type_filter_uses_binding_category_before_group_name() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-candidates-binding-category");

        let claude_key = scheduler_group_candidate_with_category(
            &database,
            &station.id,
            "plus claude key",
            "plus",
            "hash-plus-claude",
            0,
            Some("gpt"),
            Some("claude"),
        );
        scheduler_group_candidate_with_category(
            &database,
            &station.id,
            "plus inferred gpt key",
            "plus",
            "hash-plus-gpt",
            1,
            Some("gpt"),
            None,
        );

        let candidates = database
            .load_scheduler_candidates(&RoutingGroupFilter::GroupType(PricingGroupType::Claude), 0)
            .expect("scheduler candidates");

        assert_eq!(candidate_key_ids(&candidates), vec![claude_key.id]);
        assert_eq!(candidates[0].group_type, Some(PricingGroupType::Claude));
    }

    #[test]
    fn local_routing_workspace_preview_uses_bound_group_category_before_group_name() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "local-routing-binding-category");
        let key = scheduler_group_candidate_with_category(
            &database,
            &station.id,
            "plus claude local key",
            "plus",
            "hash-plus-claude-local",
            0,
            Some("gpt"),
            Some("claude"),
        );
        {
            let connection = database.connection().expect("connection");
            upsert_setting(&connection, "max_rate_multiplier", "10").expect("max multiplier");
            let filter = serialize_routing_group_filter_setting(&RoutingGroupFilter::GroupType(
                PricingGroupType::Claude,
            ))
            .expect("routing group filter");
            upsert_setting(&connection, "default_routing_group_filter", &filter)
                .expect("routing group setting");
        }

        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace");
        let candidate = workspace
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == key.id)
            .expect("candidate");

        assert!(candidate.routing_group_match);
        assert!(candidate.preview_eligible);
        assert!(!candidate
            .preview_reject_reasons
            .iter()
            .any(|reason| reason == "routing_group_mismatch"));
    }

    #[test]
    fn local_routing_workspace_uses_canonical_station_group_for_legacy_key_binding() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "local-routing-legacy-key-binding");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "legacy grouped key".to_string(),
                api_key: "sk-legacy-grouped".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: Some(true),
                group_name: Some("倍率动态调整，分组上限0.05倍率".to_string()),
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: Some(0.05),
                manual_rate_multiplier: None,
                rate_source: Some("sub2api_groups_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("key");
        {
            let connection = database.connection().expect("connection");
            migrate_legacy_group_facts(&connection).expect("legacy binding");
        }
        let canonical = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "remote-group-005".to_string(),
                group_id_hash: Some("2".to_string()),
                group_name: "倍率动态调整，分组上限0.05倍率".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.05),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.05),
                rate_source: Some("sub2api_groups_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("canonical binding");
        {
            let connection = database.connection().expect("connection");
            upsert_setting(&connection, "max_rate_multiplier", "10").expect("max multiplier");
            let filter = serialize_routing_group_filter_setting(&RoutingGroupFilter::GroupType(
                PricingGroupType::Gpt,
            ))
            .expect("routing group filter");
            upsert_setting(&connection, "default_routing_group_filter", &filter)
                .expect("routing group setting");
        }

        let stored_key = database
            .list_station_keys(station.id.clone())
            .expect("station keys")
            .into_iter()
            .find(|station_key| station_key.id == key.id)
            .expect("stored key");
        assert_ne!(
            stored_key.group_binding_id.as_deref(),
            Some(canonical.id.as_str())
        );

        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace");
        let candidate = workspace
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == key.id)
            .expect("candidate");

        assert_eq!(candidate.effective_multiplier, Some(0.05));
        assert!(candidate.routing_group_match);
        assert!(candidate.preview_eligible);
        assert!(!candidate
            .preview_reject_reasons
            .iter()
            .any(|reason| reason == "routing_group_mismatch"));
    }

    #[test]
    fn load_scheduler_candidates_uses_canonical_station_group_for_legacy_key_binding() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-legacy-key-binding");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "legacy scheduler key".to_string(),
                api_key: "sk-legacy-scheduler".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: Some(true),
                group_name: Some("倍率动态调整，分组上限0.05倍率".to_string()),
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: Some(0.05),
                manual_rate_multiplier: None,
                rate_source: Some("sub2api_groups_rates".to_string()),
                balance_scope: None,
                note: None,
            })
            .expect("key");
        {
            let connection = database.connection().expect("connection");
            migrate_legacy_group_facts(&connection).expect("legacy binding");
        }
        let canonical = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "remote-scheduler-group-005".to_string(),
                group_id_hash: Some("2".to_string()),
                group_name: "倍率动态调整，分组上限0.05倍率".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.05),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.05),
                rate_source: Some("sub2api_groups_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("canonical binding");

        let candidates = database
            .load_scheduler_candidates(&RoutingGroupFilter::GroupType(PricingGroupType::Gpt), 0)
            .expect("scheduler candidates");

        assert_eq!(candidate_key_ids(&candidates), vec![key.id]);
        assert_eq!(
            candidates[0].group_binding_id.as_deref(),
            Some(canonical.id.as_str())
        );
        assert_eq!(candidates[0].group_type, Some(PricingGroupType::Gpt));
        assert_eq!(
            candidates[0]
                .effective_multiplier
                .as_ref()
                .map(|fact| fact.value),
            Some(0.05)
        );
    }

    #[test]
    fn load_scheduler_candidates_requires_plaintext_or_encrypted_secret_material() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-candidates-secret-material");
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "UPDATE station_keys SET api_key = '', api_key_secret_id = NULL WHERE station_id = ?1",
                    params![station.id],
                )
                .expect("clear default key secret material");
        }

        let missing_secret_key = scheduler_group_candidate(
            &database,
            &station.id,
            "missing secret key",
            "gpt",
            "hash-missing-secret",
            0,
        );
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "UPDATE station_keys SET api_key = '', api_key_secret_id = NULL WHERE id = ?1",
                    params![missing_secret_key.id],
                )
                .expect("clear key secret material");
        }

        let secret_binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:secret".to_string(),
                group_id_hash: Some("hash-secret".to_string()),
                group_name: "gpt".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("group_rates".to_string()),
                confidence: 0.9,
                inferred_group_category: None,
                group_category_override: None,
                last_seen_at: None,
                raw_json_redacted: None,
            })
            .expect("secret binding");
        let data_key = [7_u8; 32];
        let encrypted_secret_key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id,
                    name: "encrypted secret key".to_string(),
                    api_key: "sk-encrypted-secret".to_string(),
                    enabled: true,
                    priority: Some(1),
                    max_concurrency: None,
                    load_factor: None,
                    schedulable: Some(true),
                    group_name: Some("gpt".to_string()),
                    tier_label: None,
                    group_binding_id: Some(secret_binding.id),
                    group_id_hash: Some("hash-secret".to_string()),
                    rate_multiplier: Some(1.0),
                    manual_rate_multiplier: None,
                    rate_source: Some("group_rates".to_string()),
                    balance_scope: None,
                    note: None,
                },
                &data_key,
            )
            .expect("encrypted secret key");

        let candidates = database
            .load_scheduler_candidates(&RoutingGroupFilter::AllGroups, 0)
            .expect("scheduler candidates");

        assert_eq!(
            candidate_key_ids(&candidates),
            vec![encrypted_secret_key.id]
        );
    }

    #[test]
    fn load_scheduler_candidates_ignores_null_key_balance_snapshots_unless_station_scoped() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-candidates-balance-scope");
        let key = scheduler_group_candidate(
            &database,
            &station.id,
            "balance scoped key",
            "gpt",
            "hash-balance-scope",
            0,
        );
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id,
                station_key_id: None,
                scope: "station_key".to_string(),
                value: Some(0.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: None,
                status: "depleted".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: Some("2026-07-11T00:00:00Z".to_string()),
            })
            .expect("balance snapshot");

        let candidates = database
            .load_scheduler_candidates(&RoutingGroupFilter::AllGroups, 0)
            .expect("scheduler candidates");

        let candidate = candidates
            .iter()
            .find(|candidate| candidate.station_key_id == key.id)
            .expect("candidate");
        assert!(!candidate.balance_depleted);
    }

    #[test]
    fn migrate_default_routing_strategy_replaces_legacy_manual_placeholder() {
        let connection = Connection::open_in_memory().expect("connection");
        initialize_schema(&connection).expect("schema");
        connection
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES ('default_routing_strategy', 'manual', '1000')",
                [],
            )
            .expect("legacy setting");

        migrate_default_routing_strategy(&connection).expect("migrate strategy");

        let value: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'default_routing_strategy'",
                [],
                |row| row.get(0),
            )
            .expect("setting value");
        assert_eq!(value, "cost_stable_first");
    }

    #[test]
    fn settings_persist_collector_proxy_defaults() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");

        let settings = database
            .update_settings(UpdateSettingsInput {
                local_proxy_port: 8787,
                default_routing_strategy: "automatic_balanced".to_string(),
                collector_proxy_mode: "manual".to_string(),
                collector_proxy_url: Some("http://127.0.0.1:7890".to_string()),
                max_rate_multiplier: None,
                default_routing_group_filter: None,
                scheduler_advanced_settings: None,
                low_balance_threshold_cny: 15.0,
                collector_interval_minutes: 30,
                balance_interval_minutes: 5,
                group_rate_interval_minutes: 20,
                model_list_interval_minutes: 60,
                pricing_refresh_interval_minutes: 60,
                collector_timeout_seconds: 15,
                collector_max_concurrency: 3,
                allow_depleted_fallback: false,
                developer_mode_enabled: false,
            })
            .expect("settings");

        assert_eq!(settings.collector_proxy_mode, "manual");
        assert_eq!(
            settings.collector_proxy_url.as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn pending_data_dir_activation_preserves_existing_database() {
        let unique = generate_id("data-dir-test");
        let root = std::env::temp_dir().join(unique);
        let default_data_dir = root.join("default");
        let custom_data_dir = root.join("custom");
        fs::create_dir_all(&default_data_dir).expect("default data dir");

        let current_db_path = default_data_dir.join(DATABASE_FILE);
        let current_connection = Connection::open(&current_db_path).expect("current db");
        initialize_schema(&current_connection).expect("schema");
        migrate_secret_schema(&current_connection).expect("secret schema");
        seed_default_settings(&current_connection).expect("settings");
        upsert_setting(&current_connection, "local_proxy_port", "9988").expect("custom settings");
        current_connection
            .execute(
                "INSERT INTO stations (
                    id, name, station_type, website_url, api_base_url, api_key,
                    enabled, priority, created_at, updated_at
                 ) VALUES ('station-imported', 'imported station', 'openai-compatible',
                    'https://example.test', 'https://example.test/v1', 'sk-test', 1, 0, '1', '1')",
                [],
            )
            .expect("station");
        drop(current_connection);

        fs::create_dir_all(&custom_data_dir).expect("custom data dir");
        let empty_custom_connection =
            Connection::open(custom_data_dir.join("relay-pool-desktop.sqlite3"))
                .expect("empty custom db");
        initialize_schema(&empty_custom_connection).expect("custom schema");
        drop(empty_custom_connection);

        let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
        write_data_dir_config(&config_path, &custom_data_dir, Some(&default_data_dir))
            .expect("write config");

        let (configured_data_dir, _pending_data_dir, source_data_dir) =
            resolve_configured_data_dir(&default_data_dir).expect("resolve configured data dir");
        let activated_db_path = prepare_configured_database(
            &default_data_dir,
            &configured_data_dir,
            source_data_dir.as_deref(),
        )
        .expect("prepare configured database");

        assert_eq!(activated_db_path, custom_data_dir.join(DATABASE_FILE));
        let activated_connection = Connection::open(&activated_db_path).expect("activated db");
        let local_proxy_port: String = activated_connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'local_proxy_port'",
                [],
                |row| row.get(0),
            )
            .expect("local proxy port");
        assert_eq!(local_proxy_port, "9988");
        let station_count: i64 = activated_connection
            .query_row("SELECT COUNT(*) FROM stations", [], |row| row.get(0))
            .expect("station count");
        assert_eq!(station_count, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_data_dir_activation_preserves_settings_without_stations() {
        let unique = generate_id("data-dir-settings-test");
        let root = std::env::temp_dir().join(unique);
        let default_data_dir = root.join("default");
        let custom_data_dir = root.join("custom");
        fs::create_dir_all(&default_data_dir).expect("default data dir");

        let current_db_path = default_data_dir.join(DATABASE_FILE);
        let current_connection = Connection::open(&current_db_path).expect("current db");
        initialize_schema(&current_connection).expect("schema");
        seed_default_settings(&current_connection).expect("settings");
        upsert_setting(&current_connection, "local_proxy_port", "9988").expect("custom port");
        drop(current_connection);

        fs::create_dir_all(&custom_data_dir).expect("custom data dir");
        let empty_custom_connection =
            Connection::open(custom_data_dir.join(DATABASE_FILE)).expect("empty custom db");
        initialize_schema(&empty_custom_connection).expect("custom schema");
        seed_default_settings(&empty_custom_connection).expect("custom defaults");
        drop(empty_custom_connection);

        let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
        write_data_dir_config(&config_path, &custom_data_dir, Some(&default_data_dir))
            .expect("write config");

        let (configured_data_dir, _pending_data_dir, source_data_dir) =
            resolve_configured_data_dir(&default_data_dir).expect("resolve configured data dir");
        let activated_db_path = prepare_configured_database(
            &default_data_dir,
            &configured_data_dir,
            source_data_dir.as_deref(),
        )
        .expect("prepare configured database");

        let activated_connection = Connection::open(&activated_db_path).expect("activated db");
        let local_proxy_port: String = activated_connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'local_proxy_port'",
                [],
                |row| row.get(0),
            )
            .expect("local proxy port");
        assert_eq!(local_proxy_port, "9988");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_data_dir_activation_does_not_overwrite_custom_target_database() {
        let unique = generate_id("data-dir-target-test");
        let root = std::env::temp_dir().join(unique);
        let source_data_dir = root.join("source");
        let custom_data_dir = root.join("custom");
        fs::create_dir_all(&source_data_dir).expect("source data dir");
        fs::create_dir_all(&custom_data_dir).expect("custom data dir");

        let source_connection =
            Connection::open(source_data_dir.join(DATABASE_FILE)).expect("source db");
        initialize_schema(&source_connection).expect("source schema");
        seed_default_settings(&source_connection).expect("source settings");
        upsert_setting(&source_connection, "local_proxy_port", "9988").expect("source port");
        drop(source_connection);

        let target_connection =
            Connection::open(custom_data_dir.join(DATABASE_FILE)).expect("target db");
        initialize_schema(&target_connection).expect("target schema");
        seed_default_settings(&target_connection).expect("target settings");
        upsert_setting(&target_connection, "local_proxy_port", "8899").expect("target port");
        drop(target_connection);

        let activated_db_path =
            prepare_configured_database(&source_data_dir, &custom_data_dir, Some(&source_data_dir))
                .expect("prepare configured database");

        let activated_connection = Connection::open(&activated_db_path).expect("activated db");
        let local_proxy_port: String = activated_connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'local_proxy_port'",
                [],
                |row| row.get(0),
            )
            .expect("local proxy port");
        assert_eq!(local_proxy_port, "8899");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn station_proxy_override_flows_into_route_candidates() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "proxied station".to_string(),
                station_type: "openai-compatible".to_string(),
                website_url: "https://proxied.example".to_string(),
                api_base_url: "https://proxied.example/v1".to_string(),
                api_key: "sk-test-routing".to_string(),
                collector_proxy_mode: "manual".to_string(),
                collector_proxy_url: Some("http://127.0.0.1:7890".to_string()),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");

        let candidates = database.proxy_route_candidates().expect("route candidates");
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.station_id == station.id)
            .expect("station candidate");

        assert_eq!(candidate.collector_proxy_mode, "manual");
        assert_eq!(
            candidate.collector_proxy_url.as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn migrate_automatic_scheduler_schema_sets_disabled_keys_unschedulable() {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                "
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    station_type TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    credit_per_cny REAL NOT NULL DEFAULT 1,
                    collection_interval_minutes INTEGER NOT NULL DEFAULT 5,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE station_keys (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO stations (
                    id, name, station_type, base_url, enabled, priority, credit_per_cny,
                    collection_interval_minutes, status, created_at, updated_at
                ) VALUES
                    ('station-enabled', 'enabled', 'sub2api', 'https://example.test', 1, 0, 1, 5, 'unchecked', '1000', '1000'),
                    ('station-disabled', 'disabled', 'sub2api', 'https://example.test', 0, 1, 1, 5, 'unchecked', '1000', '1000');
                INSERT INTO station_keys (
                    id, station_id, name, api_key, enabled, priority, status, created_at, updated_at
                ) VALUES
                    ('key-enabled', 'station-enabled', 'enabled key', 'sk-a', 1, 0, 'unchecked', '1000', '1000'),
                    ('key-disabled', 'station-disabled', 'disabled key', 'sk-b', 0, 0, 'unchecked', '1000', '1000');
                ",
            )
            .expect("legacy schema");

        migrate_automatic_scheduler_schema(&connection).expect("migrate scheduler schema");

        let enabled_schedulable: i64 = connection
            .query_row(
                "SELECT schedulable FROM station_keys WHERE id = 'key-enabled'",
                [],
                |row| row.get(0),
            )
            .expect("enabled schedulable");
        let disabled_schedulable: i64 = connection
            .query_row(
                "SELECT schedulable FROM station_keys WHERE id = 'key-disabled'",
                [],
                |row| row.get(0),
            )
            .expect("disabled schedulable");

        assert_eq!(enabled_schedulable, 1);
        assert_eq!(disabled_schedulable, 0);
    }

    #[test]
    fn migrate_automatic_scheduler_schema_preserves_later_schedulable_user_edit() {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                "
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    station_type TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    credit_per_cny REAL NOT NULL DEFAULT 1,
                    collection_interval_minutes INTEGER NOT NULL DEFAULT 5,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE station_keys (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO stations (
                    id, name, station_type, base_url, enabled, priority, credit_per_cny,
                    collection_interval_minutes, status, created_at, updated_at
                ) VALUES
                    ('station-disabled', 'disabled', 'sub2api', 'https://example.test', 0, 1, 1, 5, 'unchecked', '1000', '1000');
                INSERT INTO station_keys (
                    id, station_id, name, api_key, enabled, priority, status, created_at, updated_at
                ) VALUES
                    ('key-disabled', 'station-disabled', 'disabled key', 'sk-b', 0, 0, 'unchecked', '1000', '1000');
                ",
            )
            .expect("legacy schema");

        migrate_automatic_scheduler_schema(&connection).expect("initial scheduler migration");
        connection
            .execute(
                "UPDATE station_keys SET schedulable = 1 WHERE id = 'key-disabled'",
                [],
            )
            .expect("simulate user schedulable edit");

        migrate_automatic_scheduler_schema(&connection).expect("repeat scheduler migration");

        let schedulable: i64 = connection
            .query_row(
                "SELECT schedulable FROM station_keys WHERE id = 'key-disabled'",
                [],
                |row| row.get(0),
            )
            .expect("schedulable");

        assert_eq!(schedulable, 1);
    }

    #[test]
    fn station_endpoint_health_flows_into_key_pool_items() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "endpoint-health-relay");

        let default_health = database
            .get_station_endpoint_health(station.id.clone())
            .expect("default endpoint health");
        assert_eq!(default_health.status, "unchecked");
        assert_eq!(default_health.latency_ms, None);

        database
            .upsert_station_endpoint_health(&station.id, "success", Some(38), "1000", None)
            .expect("endpoint health");

        let key_pool_item = database
            .list_key_pool_items()
            .expect("key pool")
            .into_iter()
            .find(|item| item.station_id == station.id)
            .expect("station key pool item");
        assert_eq!(key_pool_item.endpoint_ping_status, "success");
        assert_eq!(key_pool_item.endpoint_ping_ms, Some(38));
        assert_eq!(
            key_pool_item.endpoint_ping_checked_at.as_deref(),
            Some("1000")
        );
        assert_eq!(key_pool_item.endpoint_ping_error, None);
    }

    #[test]
    fn create_station_without_api_key_creates_station_without_default_key() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [7_u8; 32];

        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "login only station".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: "https://relay.example.test".to_string(),
                    api_base_url: "https://relay.example.test/v1".to_string(),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station without api key");
        let keys = database
            .list_station_keys(station.id.clone())
            .expect("station keys");

        assert!(!station.api_key_present);
        assert!(keys.is_empty());
    }

    #[test]
    fn station_key_create_accepts_unlimited_max_concurrency_and_valid_ranges() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-ranges");

        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "scheduler key".to_string(),
                api_key: "sk-scheduler".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: Some(0),
                load_factor: Some(10_000),
                schedulable: Some(true),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: Some(1.25),
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("station key");

        assert_eq!(key.max_concurrency, 0);
        assert_eq!(key.load_factor, Some(10_000));
        assert_eq!(key.manual_rate_multiplier, Some(1.25));
    }

    #[test]
    fn station_key_create_rejects_invalid_load_factor() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-load-factor");

        let error = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "scheduler key".to_string(),
                api_key: "sk-scheduler".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: Some(1),
                load_factor: Some(0),
                schedulable: Some(true),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect_err("load factor must be rejected");

        assert!(error.contains("load_factor"));
    }

    #[test]
    fn station_key_create_rejects_invalid_manual_rate_multiplier() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-manual-rate-invalid");

        let error = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "scheduler key".to_string(),
                api_key: "sk-scheduler".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: Some(1),
                load_factor: Some(1),
                schedulable: Some(true),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: Some(-1.0),
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect_err("manual multiplier must be rejected");

        assert!(error.contains("manual_rate_multiplier"));
    }

    #[test]
    fn station_key_update_can_clear_manual_rate_multiplier() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scheduler-manual-rate");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "scheduler key".to_string(),
                api_key: "sk-scheduler".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: Some(1),
                load_factor: Some(1),
                schedulable: Some(true),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: Some(2.5),
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("station key");

        let cleared = database
            .update_station_key(UpdateStationKeyInput {
                id: key.id.clone(),
                station_id: station.id,
                name: key.name,
                api_key: None,
                enabled: key.enabled,
                priority: key.priority,
                max_concurrency: key.max_concurrency,
                load_factor: key.load_factor,
                schedulable: key.schedulable,
                group_name: key.group_name,
                tier_label: key.tier_label,
                group_binding_id: key.group_binding_id,
                group_id_hash: key.group_id_hash,
                rate_multiplier: key.rate_multiplier,
                manual_rate_multiplier: Some(None),
                rate_source: key.rate_source,
                balance_scope: key.balance_scope,
                status: key.status,
                note: key.note,
            })
            .expect("clear manual rate");

        assert_eq!(cleared.manual_rate_multiplier, None);
        assert!(cleared.manual_rate_updated_at.is_some());
    }

    #[test]
    fn due_station_collectors_use_station_collection_interval() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let fresh = test_station(&database, "fresh station");
        let stale = test_station(&database, "stale station");
        let never_collected = test_station(&database, "never collected station");
        let now = now_millis_for_services() as i64;
        let fresh_checked_at = (now - 4 * 60 * 1000).to_string();
        let stale_checked_at = (now - 6 * 60 * 1000).to_string();
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "UPDATE stations SET last_pricing_fetched_at = ?1 WHERE id = ?2",
                    params![fresh_checked_at, fresh.id],
                )
                .expect("fresh station timestamp");
            connection
                .execute(
                    "UPDATE stations SET last_pricing_fetched_at = ?1 WHERE id = ?2",
                    params![stale_checked_at, stale.id],
                )
                .expect("stale station timestamp");
        }

        let due = database
            .due_station_collectors(&now.to_string())
            .expect("due station collectors");
        let due_ids = due
            .iter()
            .map(|station| station.id.as_str())
            .collect::<Vec<_>>();

        assert!(!due_ids.contains(&fresh.id.as_str()));
        assert!(due_ids.contains(&stale.id.as_str()));
        assert!(due_ids.contains(&never_collected.id.as_str()));
    }

    #[test]
    fn channel_monitor_builtin_template_seeding_is_idempotent() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");

        database
            .seed_builtin_channel_monitor_templates()
            .expect("seed again");
        let templates = database
            .list_channel_monitor_templates()
            .expect("templates");
        let built_in_ids: Vec<_> = templates
            .iter()
            .filter(|template| template.built_in)
            .map(|template| template.id.as_str())
            .collect();

        assert_eq!(built_in_ids.len(), 4);
        for expected_id in [
            "builtin-openai-chat-default",
            "builtin-openai-chat-low-token",
            "builtin-openai-responses-default",
            "builtin-openai-responses-low-token",
        ] {
            assert!(
                built_in_ids.contains(&expected_id),
                "{expected_id} should be seeded"
            );
        }
        for template in templates.iter().filter(|template| template.built_in) {
            let body: Value =
                serde_json::from_str(&template.request_body_json).expect("built-in request body");
            assert_eq!(body["stream"], "{{stream}}", "{} stream", template.id);
            if template.endpoint_kind == "responses" {
                assert_eq!(
                    body.pointer("/reasoning/effort").and_then(Value::as_str),
                    Some("minimal"),
                    "{} reasoning effort",
                    template.id
                );
            }
        }
        let responses_low_token = templates
            .iter()
            .find(|template| template.id == "builtin-openai-responses-low-token")
            .expect("responses low token template");
        let responses_body: Value =
            serde_json::from_str(&responses_low_token.request_body_json).expect("responses body");
        assert_eq!(responses_body["instructions"], "Reply with OK only.");
        assert_eq!(responses_body["input"], "{{challenge}}");
        assert_eq!(responses_body["max_output_tokens"], 1);
        assert_eq!(responses_body["store"], false);
        assert_eq!(responses_body["stream"], "{{stream}}");
    }

    #[test]
    fn create_key_channel_monitor_round_trips_fallback_models() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "monitor key station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "key smoke".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id.clone(),
                station_key_id: Some(key.id.clone()),
                template_id: "builtin-openai-chat-default".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 10,
                timeout_seconds: 15,
                max_concurrency: 2,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-4o-mini".to_string(), "gpt-4.1-mini".to_string()],
                note: Some("round trip".to_string()),
            })
            .expect("create monitor");
        let monitors = database.list_channel_monitors().expect("monitors");
        let saved = monitors
            .iter()
            .find(|item| item.id == monitor.id)
            .expect("saved monitor");

        assert_eq!(saved.target_type, "station_key");
        assert_eq!(saved.station_id, station.id);
        assert_eq!(saved.station_key_id.as_deref(), Some(key.id.as_str()));
        assert_eq!(
            saved.fallback_models,
            vec!["gpt-4o-mini".to_string(), "gpt-4.1-mini".to_string()]
        );
    }

    #[test]
    fn channel_monitor_rejects_station_key_mismatch() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "monitor owning station");
        let other_station = test_station(&database, "monitor other station");
        let other_key = database
            .list_station_keys(other_station.id)
            .expect("keys")
            .remove(0);

        let error = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "bad key".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(other_key.id),
                template_id: "builtin-openai-chat-default".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 15,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect_err("mismatch rejected");

        assert!(error.contains("Station key does not belong to station"));
    }

    #[test]
    fn deleting_referenced_channel_monitor_template_is_rejected() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "template reference station");
        let template = database
            .create_channel_monitor_template(CreateChannelMonitorTemplateInput {
                name: "custom template".to_string(),
                endpoint_kind: "chat_completions".to_string(),
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                request_body_json: r#"{"model":"gpt-4o-mini","messages":[]}"#.to_string(),
                enabled: true,
                note: None,
            })
            .expect("template");
        database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "station smoke".to_string(),
                target_type: "station".to_string(),
                station_id: station.id,
                station_key_id: None,
                template_id: template.id.clone(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 15,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("monitor");

        let error = database
            .delete_channel_monitor_template(template.id)
            .expect_err("referenced template rejected");

        assert!(error.contains("referenced by channel monitors"));
    }

    #[test]
    fn duplicating_builtin_channel_monitor_template_creates_editable_copy() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");

        let copy = database
            .duplicate_channel_monitor_template("builtin-openai-chat-default".to_string())
            .expect("duplicate");
        assert!(!copy.built_in);
        assert_ne!(copy.id, "builtin-openai-chat-default");

        let updated = database
            .update_channel_monitor_template(UpdateChannelMonitorTemplateInput {
                id: copy.id.clone(),
                name: "custom editable copy".to_string(),
                endpoint_kind: copy.endpoint_kind,
                method: copy.method,
                path: copy.path,
                request_body_json: copy.request_body_json,
                enabled: true,
                note: Some("edited".to_string()),
            })
            .expect("update custom copy");

        assert_eq!(updated.name, "custom editable copy");
        assert_eq!(updated.note.as_deref(), Some("edited"));
    }

    #[test]
    fn channel_monitor_template_rejects_external_origin_path() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");

        let error = database
            .create_channel_monitor_template(CreateChannelMonitorTemplateInput {
                name: "external path".to_string(),
                endpoint_kind: "chat_completions".to_string(),
                method: "POST".to_string(),
                path: "//example.com/v1/chat/completions".to_string(),
                request_body_json: r#"{"model":"gpt-4o-mini","messages":[]}"#.to_string(),
                enabled: true,
                note: None,
            })
            .expect_err("external origin path rejected");

        assert!(error.contains("same-origin"));
    }

    #[test]
    fn channel_monitor_run_rejects_invalid_status() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = test_channel_monitor(&database, "invalid run status");

        let error = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id,
                template_id: monitor.template_id,
                station_id: monitor.station_id,
                station_key_id: monitor.station_key_id,
                status: "running".to_string(),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(95),
                response_model: Some("gpt-4o-mini".to_string()),
                fallback_model: None,
                error_message: None,
            })
            .expect_err("invalid status rejected");

        assert!(error.contains("status must be success, warning, failed, or skipped"));
    }

    #[test]
    fn channel_monitor_run_rejects_negative_latency_and_bad_status_code() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = test_channel_monitor(&database, "invalid run metrics");

        let negative_latency = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id.clone(),
                template_id: monitor.template_id.clone(),
                station_id: monitor.station_id.clone(),
                station_key_id: monitor.station_key_id.clone(),
                status: "success".to_string(),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(-1),
                response_model: None,
                fallback_model: None,
                error_message: None,
            })
            .expect_err("negative latency rejected");
        assert!(negative_latency.contains("latency_ms must be non-negative"));

        let bad_status = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id,
                template_id: monitor.template_id,
                station_id: monitor.station_id,
                station_key_id: monitor.station_key_id,
                status: "failed".to_string(),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
                http_status: Some(42),
                latency_ms: Some(10),
                response_model: None,
                fallback_model: None,
                error_message: Some("bad upstream".to_string()),
            })
            .expect_err("bad status code rejected");
        assert!(bad_status.contains("status_code must be between 100 and 599"));
    }

    #[test]
    fn channel_monitor_run_rejects_non_numeric_started_at() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = test_channel_monitor(&database, "invalid started at");

        let error = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id,
                template_id: monitor.template_id,
                station_id: monitor.station_id,
                station_key_id: monitor.station_key_id,
                status: "success".to_string(),
                started_at: "not-a-time".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(95),
                response_model: None,
                fallback_model: None,
                error_message: None,
            })
            .expect_err("invalid started_at rejected");

        assert!(error.contains("started_at must be a positive millisecond epoch"));
    }

    #[test]
    fn channel_monitor_run_rejects_non_numeric_finished_at() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = test_channel_monitor(&database, "invalid finished at");

        let error = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id,
                template_id: monitor.template_id,
                station_id: monitor.station_id,
                station_key_id: monitor.station_key_id,
                status: "success".to_string(),
                started_at: "1000".to_string(),
                finished_at: Some("not-a-time".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(95),
                response_model: None,
                fallback_model: None,
                error_message: None,
            })
            .expect_err("invalid finished_at rejected");

        assert!(error.contains("finished_at must be a positive millisecond epoch"));
    }

    #[test]
    fn channel_monitor_run_rejects_finished_at_before_started_at() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = test_channel_monitor(&database, "finished before started");

        let error = database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id,
                template_id: monitor.template_id,
                station_id: monitor.station_id,
                station_key_id: monitor.station_key_id,
                status: "warning".to_string(),
                started_at: "2000".to_string(),
                finished_at: Some("1000".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(95),
                response_model: None,
                fallback_model: None,
                error_message: None,
            })
            .expect_err("finished_at before started_at rejected");

        assert!(error.contains("finished_at cannot be earlier than started_at"));
    }

    #[test]
    fn channel_monitor_table_rejects_invalid_target_shape() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "db target shape");
        let connection = database.connection().expect("connection");

        let result = connection.execute(
            "INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                note, created_at, updated_at
             ) VALUES (
                'bad-target-shape', 'bad target shape', 'station_key', ?1, NULL,
                'builtin-openai-chat-default', 1, 60, 0, 15, 1, 3, '[]',
                NULL, '1000', '1000'
             )",
            params![station.id],
        );

        assert!(result.is_err(), "invalid target shape should fail CHECK");
    }

    #[test]
    fn remote_station_keys_replace_per_station() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Remote Key Station");

        let saved = database
            .replace_remote_station_keys(
                station.id.clone(),
                vec![crate::models::remote_keys::RemoteStationKey {
                    id: "remote-key-1".to_string(),
                    station_id: station.id.clone(),
                    remote_key_id_hash: Some("remote-id-hash".to_string()),
                    remote_key_name: Some("remote key".to_string()),
                    api_key_masked: Some("sk-****abcd".to_string()),
                    api_key_fingerprint: Some("fingerprint".to_string()),
                    group_id_hash: Some("group-hash".to_string()),
                    group_name: Some("pro".to_string()),
                    tier_label: Some("tier-a".to_string()),
                    rate_multiplier: Some(0.75),
                    rate_source: Some("remote".to_string()),
                    created_at: Some("1000".to_string()),
                    last_used_at: Some("2000".to_string()),
                    raw_source: "sub2api".to_string(),
                    match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
                    matched_station_key_id: None,
                    match_confidence: 0.0,
                    collected_at: "3000".to_string(),
                }],
            )
            .expect("replace remote keys");

        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].group_name.as_deref(), Some("pro"));
        assert_eq!(saved[0].rate_multiplier, Some(0.75));

        let listed = database
            .list_remote_station_keys(station.id.clone())
            .expect("list remote keys");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].group_name.as_deref(), Some("pro"));
        assert_eq!(listed[0].rate_multiplier, Some(0.75));

        let cleared = database
            .replace_remote_station_keys(station.id.clone(), Vec::new())
            .expect("clear remote keys");
        assert!(cleared.is_empty());
        assert!(database
            .list_remote_station_keys(station.id)
            .expect("list after clear")
            .is_empty());
    }

    #[test]
    fn remote_station_key_bind_marks_match() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Remote Bind Station");
        let station_key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "local key".to_string(),
                api_key: "sk-local-bind".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("station key");

        database
            .replace_remote_station_keys(
                station.id.clone(),
                vec![crate::models::remote_keys::RemoteStationKey {
                    id: "remote-bind-1".to_string(),
                    station_id: station.id.clone(),
                    remote_key_id_hash: Some("remote-bind-hash".to_string()),
                    remote_key_name: Some("remote bind key".to_string()),
                    api_key_masked: None,
                    api_key_fingerprint: None,
                    group_id_hash: None,
                    group_name: None,
                    tier_label: None,
                    rate_multiplier: None,
                    rate_source: None,
                    created_at: None,
                    last_used_at: None,
                    raw_source: "sub2api".to_string(),
                    match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
                    matched_station_key_id: None,
                    match_confidence: 0.0,
                    collected_at: "3000".to_string(),
                }],
            )
            .expect("insert remote key");

        let keys = database
            .bind_remote_station_key("remote-bind-1".to_string(), station_key.id.clone())
            .expect("bind remote key");
        let matched = keys
            .into_iter()
            .find(|key| key.id == "remote-bind-1")
            .expect("matched remote key");

        assert_eq!(
            matched.match_status,
            crate::models::remote_keys::RemoteKeyMatchStatus::Matched
        );
        assert_eq!(
            matched.matched_station_key_id.as_deref(),
            Some(station_key.id.as_str())
        );
        assert_eq!(matched.match_confidence, 1.0);
    }

    #[test]
    fn remote_station_keys_are_removed_when_station_is_deleted() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Remote Cascade Station");

        database
            .replace_remote_station_keys(
                station.id.clone(),
                vec![crate::models::remote_keys::RemoteStationKey {
                    id: "remote-cascade-1".to_string(),
                    station_id: station.id.clone(),
                    remote_key_id_hash: Some("remote-cascade-hash".to_string()),
                    remote_key_name: Some("remote cascade key".to_string()),
                    api_key_masked: None,
                    api_key_fingerprint: None,
                    group_id_hash: None,
                    group_name: None,
                    tier_label: None,
                    rate_multiplier: None,
                    rate_source: None,
                    created_at: None,
                    last_used_at: None,
                    raw_source: "sub2api".to_string(),
                    match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
                    matched_station_key_id: None,
                    match_confidence: 0.0,
                    collected_at: "3000".to_string(),
                }],
            )
            .expect("insert remote key");

        database
            .delete_station(station.id.clone())
            .expect("delete station");

        let connection = database.connection().expect("connection");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM remote_station_keys WHERE station_id = ?1",
                params![station.id],
                |row| row.get(0),
            )
            .expect("remote key count");
        assert_eq!(count, 0);
    }

    #[test]
    fn remote_station_key_is_unmatched_when_station_key_is_deleted() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Remote Unmatch Station");
        let station_key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "local matched key".to_string(),
                api_key: "sk-local-unmatch".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("station key");

        database
            .replace_remote_station_keys(
                station.id.clone(),
                vec![crate::models::remote_keys::RemoteStationKey {
                    id: "remote-unmatch-1".to_string(),
                    station_id: station.id.clone(),
                    remote_key_id_hash: Some("remote-unmatch-hash".to_string()),
                    remote_key_name: Some("remote unmatch key".to_string()),
                    api_key_masked: None,
                    api_key_fingerprint: None,
                    group_id_hash: None,
                    group_name: None,
                    tier_label: None,
                    rate_multiplier: None,
                    rate_source: None,
                    created_at: None,
                    last_used_at: None,
                    raw_source: "sub2api".to_string(),
                    match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
                    matched_station_key_id: None,
                    match_confidence: 0.0,
                    collected_at: "3000".to_string(),
                }],
            )
            .expect("insert remote key");
        database
            .bind_remote_station_key("remote-unmatch-1".to_string(), station_key.id.clone())
            .expect("bind remote key");

        database
            .delete_station_key(station_key.id)
            .expect("delete station key");
        let remote_key = database
            .list_remote_station_keys(station.id)
            .expect("remote keys")
            .into_iter()
            .find(|key| key.id == "remote-unmatch-1")
            .expect("remote key remains");

        assert_eq!(
            remote_key.match_status,
            crate::models::remote_keys::RemoteKeyMatchStatus::Unbound
        );
        assert_eq!(remote_key.matched_station_key_id, None);
        assert_eq!(remote_key.match_confidence, 0.0);
    }

    #[test]
    fn remote_station_key_replace_sanitizes_cross_station_match() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Remote Replace Station");
        let other_station = test_station(&database, "Other Replace Station");
        let other_station_key = database
            .create_station_key(CreateStationKeyInput {
                station_id: other_station.id,
                name: "foreign local key".to_string(),
                api_key: "sk-foreign-local".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("foreign station key");

        let saved = database
            .replace_remote_station_keys(
                station.id,
                vec![crate::models::remote_keys::RemoteStationKey {
                    id: "remote-cross-replace".to_string(),
                    station_id: "ignored-station".to_string(),
                    remote_key_id_hash: Some("remote-cross-replace-hash".to_string()),
                    remote_key_name: Some("remote cross replace".to_string()),
                    api_key_masked: None,
                    api_key_fingerprint: None,
                    group_id_hash: None,
                    group_name: None,
                    tier_label: None,
                    rate_multiplier: None,
                    rate_source: None,
                    created_at: None,
                    last_used_at: None,
                    raw_source: "sub2api".to_string(),
                    match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Matched,
                    matched_station_key_id: Some(other_station_key.id),
                    match_confidence: 1.0,
                    collected_at: "3000".to_string(),
                }],
            )
            .expect("replace remote keys");

        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].match_status,
            crate::models::remote_keys::RemoteKeyMatchStatus::Unbound
        );
        assert_eq!(saved[0].matched_station_key_id, None);
        assert_eq!(saved[0].match_confidence, 0.0);
    }

    #[test]
    fn remote_station_key_migration_rebuilds_old_table_with_foreign_keys() {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    station_type TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    upstream_api_format TEXT NOT NULL DEFAULT 'auto',
                    upstream_api_base_path TEXT NOT NULL DEFAULT '/v1',
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    credit_per_cny REAL NOT NULL DEFAULT 1,
                    balance_raw REAL,
                    balance_cny REAL,
                    low_balance_threshold_cny REAL,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    latency_ms INTEGER,
                    last_checked_at TEXT,
                    last_pricing_fetched_at TEXT,
                    note TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE station_keys (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    group_name TEXT,
                    tier_label TEXT,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    last_checked_at TEXT,
                    last_used_at TEXT,
                    note TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
                );

                CREATE TABLE remote_station_keys (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    remote_key_id_hash TEXT,
                    remote_key_name TEXT,
                    api_key_masked TEXT,
                    api_key_fingerprint TEXT,
                    group_id_hash TEXT,
                    group_name TEXT,
                    tier_label TEXT,
                    rate_multiplier REAL,
                    rate_source TEXT,
                    created_at_remote TEXT,
                    last_used_at TEXT,
                    raw_source TEXT NOT NULL,
                    match_status TEXT NOT NULL,
                    matched_station_key_id TEXT,
                    match_confidence REAL NOT NULL,
                    collected_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                INSERT INTO stations (
                    id, name, station_type, base_url, api_key, created_at, updated_at
                ) VALUES (
                    'station-old', 'Old Station', 'openai-compatible',
                    'https://example.test', 'sk-test', '1000', '1000'
                ), (
                    'station-other', 'Other Station', 'openai-compatible',
                    'https://other.example.test', 'sk-other', '1000', '1000'
                );

                INSERT INTO station_keys (
                    id, station_id, name, api_key, created_at, updated_at
                ) VALUES (
                    'key-old', 'station-old', 'Old Key', 'sk-local', '1000', '1000'
                ), (
                    'key-cross-station', 'station-other', 'Cross Station Key',
                    'sk-cross', '1000', '1000'
                );

                INSERT INTO remote_station_keys (
                    id, station_id, remote_key_id_hash, remote_key_name,
                    api_key_masked, api_key_fingerprint, group_id_hash, group_name,
                    tier_label, rate_multiplier, rate_source, created_at_remote,
                    last_used_at, raw_source, match_status, matched_station_key_id,
                    match_confidence, collected_at, updated_at
                ) VALUES (
                    'remote-old', 'station-old', 'remote-hash', 'Remote Old',
                    'sk-****old', 'fingerprint-old', 'group-hash', 'pro',
                    'tier-a', 0.75, 'legacy', '900', '950', 'sub2api',
                    'matched', 'key-old', 1.0, '1100', '1100'
                ), (
                    'remote-stale-binding', 'station-old', 'remote-stale-hash',
                    'Remote Stale', NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                    NULL, NULL, 'sub2api', 'matched', 'missing-key', 1.0,
                    '1200', '1200'
                ), (
                    'remote-cross-station', 'station-old', 'remote-cross-hash',
                    'Remote Cross Station', NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                    NULL, NULL, 'sub2api', 'matched', 'key-cross-station', 1.0,
                    '1300', '1300'
                );
                "#,
            )
            .expect("old schema");

        migrate_remote_key_tables(&connection).expect("migrate remote keys");

        let mut foreign_keys = connection
            .prepare("PRAGMA foreign_key_list(remote_station_keys)")
            .expect("foreign key list");
        let references = foreign_keys
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .expect("query foreign keys")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect foreign keys");
        assert!(references.iter().any(|(from, table, on_delete)| {
            from == "station_id" && table == "stations" && on_delete == "CASCADE"
        }));
        assert!(references.iter().any(|(from, table, on_delete)| {
            from == "matched_station_key_id" && table == "station_keys" && on_delete == "SET NULL"
        }));

        let remote_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM remote_station_keys", [], |row| {
                row.get(0)
            })
            .expect("remote count");
        assert_eq!(remote_count, 3);

        let stale_binding: (String, Option<String>, f64) = connection
            .query_row(
                "SELECT match_status, matched_station_key_id, match_confidence
                   FROM remote_station_keys
                  WHERE id = 'remote-stale-binding'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("stale binding");
        assert_eq!(stale_binding.0, "unbound");
        assert_eq!(stale_binding.1, None);
        assert_eq!(stale_binding.2, 0.0);

        let cross_station_binding: (String, Option<String>, f64) = connection
            .query_row(
                "SELECT match_status, matched_station_key_id, match_confidence
                   FROM remote_station_keys
                  WHERE id = 'remote-cross-station'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("cross station binding");
        assert_eq!(cross_station_binding.0, "unbound");
        assert_eq!(cross_station_binding.1, None);
        assert_eq!(cross_station_binding.2, 0.0);

        connection
            .execute("DELETE FROM stations WHERE id = 'station-old'", [])
            .expect("delete station");
        let remaining_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM remote_station_keys", [], |row| {
                row.get(0)
            })
            .expect("remaining remote count");
        assert_eq!(remaining_count, 0);
    }

    #[test]
    fn routing_tables_exist_in_new_database() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let connection = database.connection().expect("connection");

        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                    'station_key_capabilities',
                    'model_aliases',
                    'station_key_health'
                )",
                [],
                |row| row.get(0),
            )
            .expect("table count");

        assert_eq!(count, 3);
    }

    #[test]
    fn p9_fact_tables_are_initialized() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let connection = database.connection().expect("connection");

        for table in [
            "station_group_bindings",
            "group_rate_records",
            "collector_runs",
        ] {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .expect("table count");
            assert_eq!(count, 1, "{table} should exist");
        }
    }

    #[test]
    fn key_pool_reads_manual_group_binding_facts() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "Binding Facts");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .into_iter()
            .next()
            .expect("default key");
        let binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "group-hash-pro".to_string(),
                group_id_hash: Some("external-group-hash".to_string()),
                group_name: "pro".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(0.72),
                effective_rate_multiplier: Some(0.72),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: None,
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("binding");

        let saved = database
            .update_station_key_group_binding(UpdateStationKeyGroupBindingInput {
                station_key_id: key.id.clone(),
                group_binding_id: binding.id.clone(),
            })
            .expect("bind key");

        assert_eq!(saved.group_binding_id.as_deref(), Some(binding.id.as_str()));
        assert_eq!(saved.group_name.as_deref(), Some("pro"));
        assert_eq!(saved.group_id_hash.as_deref(), Some("group-hash-pro"));
        assert_eq!(saved.rate_multiplier, Some(0.72));
        assert_eq!(saved.rate_source.as_deref(), Some("manual"));
        assert_eq!(saved.balance_scope.as_deref(), Some("station"));

        let item = database
            .list_key_pool_items()
            .expect("key pool")
            .into_iter()
            .find(|item| item.id == key.id)
            .expect("key pool item");
        assert_eq!(item.group_binding_id.as_deref(), Some(binding.id.as_str()));
        assert_eq!(item.rate_multiplier, Some(0.72));
        assert_eq!(item.balance_scope.as_deref(), Some("station"));
    }

    #[test]
    fn key_pool_items_include_station_upstream_api_format() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "responses station".to_string(),
                station_type: "openai-compatible".to_string(),
                website_url: "https://responses.example".to_string(),
                api_base_url: "https://responses.example/v1".to_string(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-responses".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "UPDATE stations SET upstream_api_format = 'openai_responses' WHERE id = ?1",
                    params![station.id],
                )
                .expect("set upstream format");
        }

        let item = database.list_key_pool_items().expect("key pool").remove(0);

        assert_eq!(item.station_upstream_api_format, "openai_responses");
    }

    #[test]
    fn p9_extension_columns_are_initialized() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let connection = database.connection().expect("connection");

        for (table, column) in [
            ("station_credentials", "access_token_secret_id"),
            ("station_credentials", "refresh_token_secret_id"),
            ("station_credentials", "cookie_secret_id"),
            ("station_credentials", "newapi_user_id"),
            ("station_credentials", "session_source"),
            ("station_keys", "group_binding_id"),
            ("station_keys", "group_id_hash"),
            ("station_keys", "rate_multiplier"),
            ("station_keys", "rate_collected_at"),
            ("station_keys", "balance_scope"),
            ("pricing_rules", "station_key_id"),
            ("pricing_rules", "group_binding_id"),
            ("pricing_rules", "rate_multiplier"),
            ("pricing_rules", "normalization_status"),
            ("pricing_rules", "valid_until"),
            ("request_logs", "group_binding_id"),
            ("request_logs", "normalization_status"),
            ("request_logs", "balance_scope"),
            ("request_logs", "economic_context_json"),
            ("request_logs", "lifecycle_status"),
        ] {
            let count: i64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
                    params![column],
                    |row| row.get(0),
                )
                .expect("column count");
            assert_eq!(count, 1, "{table}.{column} should exist");
        }
    }

    #[test]
    fn station_detail_balance_snapshots_are_filtered_in_database() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let target = test_station(&database, "Target Station");
        let other = test_station(&database, "Other Station");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("balance-target".to_string()),
                station_id: target.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(10.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: None,
                status: "normal".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: None,
            })
            .expect("target balance");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("balance-other".to_string()),
                station_id: other.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(99.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: None,
                status: "normal".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: None,
            })
            .expect("other balance");

        let snapshots = database
            .list_balance_snapshots_for_station(target.id.clone())
            .expect("station balance snapshots");

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "balance-target");
    }

    #[test]
    fn current_station_balances_return_one_latest_station_scope_row_per_station() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station_a = test_station(&database, "Station A");
        let station_b = test_station(&database, "Station B");
        let connection = database.connection().expect("connection");

        for (id, station_id, scope, value, created_at, updated_at) in [
            (
                "a-old",
                station_a.id.as_str(),
                "station",
                10.0,
                "2026-07-13T01:00:00Z",
                "2026-07-13T01:00:00Z",
            ),
            (
                "a-same-updated-older-created",
                station_a.id.as_str(),
                "station",
                11.0,
                "2026-07-13T01:30:00Z",
                "2026-07-13T02:00:00Z",
            ),
            (
                "a-new",
                station_a.id.as_str(),
                "station",
                12.0,
                "2026-07-13T02:00:00Z",
                "2026-07-13T02:00:00Z",
            ),
            (
                "a-key",
                station_a.id.as_str(),
                "station_key",
                99.0,
                "2026-07-13T03:00:00Z",
                "2026-07-13T03:00:00Z",
            ),
            (
                "b-current",
                station_b.id.as_str(),
                "station",
                8.0,
                "2026-07-13T01:30:00Z",
                "2026-07-13T01:30:00Z",
            ),
            (
                "b-current-z",
                station_b.id.as_str(),
                "station",
                9.0,
                "2026-07-13T01:30:00Z",
                "2026-07-13T01:30:00Z",
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO balance_snapshots (
                        id, station_id, station_key_id, scope, value, currency, status,
                        source, confidence, created_at, updated_at
                     ) VALUES (?1, ?2, NULL, ?3, ?4, 'CNY', 'normal', 'test', 1.0, ?5, ?6)",
                    params![id, station_id, scope, value, created_at, updated_at],
                )
                .expect("insert balance snapshot");
        }
        drop(connection);

        let snapshots = database
            .list_current_station_balance_snapshots()
            .expect("current station balances");
        let ids = snapshots
            .iter()
            .map(|snapshot| snapshot.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["a-new", "b-current-z"]);
    }

    #[test]
    fn station_detail_change_events_are_filtered_in_database() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let target = test_station(&database, "Target Station");
        let other = test_station(&database, "Other Station");
        database
            .upsert_change_event(UpsertChangeEventInput {
                severity: crate::services::change_events::SEVERITY_INFO.to_string(),
                event_type: "test_event".to_string(),
                title: "Target".to_string(),
                message: "Target message".to_string(),
                object_type: "station".to_string(),
                object_id: Some(target.id.clone()),
                station_id: Some(target.id.clone()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: None,
                impact_json: None,
                dedupe_key: "event-target".to_string(),
                source: "test".to_string(),
            })
            .expect("target event");
        database
            .upsert_change_event(UpsertChangeEventInput {
                severity: crate::services::change_events::SEVERITY_INFO.to_string(),
                event_type: "test_event".to_string(),
                title: "Other".to_string(),
                message: "Other message".to_string(),
                object_type: "station".to_string(),
                object_id: Some(other.id.clone()),
                station_id: Some(other.id.clone()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: None,
                impact_json: None,
                dedupe_key: "event-other".to_string(),
                source: "test".to_string(),
            })
            .expect("other event");

        let events = database
            .list_change_events_for_station(target.id.clone())
            .expect("station change events");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].dedupe_key, "event-target");
    }

    #[test]
    fn group_binding_upsert_dedupes_without_external_group_id() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "group-relay");

        let first = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:default".to_string(),
                group_id_hash: None,
                group_name: "default".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.8,
                last_seen_at: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("first");

        let second = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:default".to_string(),
                group_id_hash: None,
                group_name: "default".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.2),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.2),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("second");

        assert_eq!(first.id, second.id);
        assert_eq!(second.effective_rate_multiplier, Some(1.2));
    }

    #[test]
    fn collector_group_upsert_disables_same_name_remote_scan_shadow_binding() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "group-shadow-relay");

        let shadow = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "frontend-remote-scan-plus".to_string(),
                group_id_hash: Some("frontend-plus".to_string()),
                group_name: "plus".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.1),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.1),
                rate_source: Some("remote_scan".to_string()),
                confidence: 0.95,
                last_seen_at: Some("1000".to_string()),
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("shadow binding");

        let canonical = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "collector-sub2api-plus".to_string(),
                group_id_hash: Some("plus".to_string()),
                group_name: "plus".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.1),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.1),
                rate_source: Some("sub2api_groups_rates".to_string()),
                confidence: 0.9,
                last_seen_at: Some("2000".to_string()),
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("canonical binding");

        let bindings = database
            .list_station_group_bindings(station.id)
            .expect("bindings");
        let shadow = bindings
            .iter()
            .find(|binding| binding.id == shadow.id)
            .expect("stored shadow binding");
        let canonical = bindings
            .iter()
            .find(|binding| binding.id == canonical.id)
            .expect("stored canonical binding");

        assert_eq!(shadow.binding_status, "disabled");
        assert_eq!(canonical.binding_status, BINDING_STATUS_AVAILABLE);
    }

    #[test]
    fn group_missing_event_is_emitted_when_available_group_disappears() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "missing-group-relay");

        let available = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:missing".to_string(),
                group_id_hash: None,
                group_name: "pro".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: Some("1000".to_string()),
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("available");
        database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:missing".to_string(),
                group_id_hash: None,
                group_name: "pro".to_string(),
                binding_status: BINDING_STATUS_MISSING.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: None,
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("missing");

        let events = database.list_change_events().expect("events");
        assert!(events.iter().any(|event| {
            event.event_type == "group_missing"
                && event.object_type == "group_binding"
                && event.object_id.as_deref() == Some(available.id.as_str())
                && event.severity == "warning"
        }));
    }

    #[test]
    fn group_added_event_includes_collected_rate_multiplier() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "group-added-rate-relay");

        database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:codex-pro".to_string(),
                group_id_hash: Some("codex-pro".to_string()),
                group_name: "codex-pro".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.25),
                effective_rate_multiplier: Some(1.25),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: Some("2026-07-09T01:00:00.000Z".to_string()),
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("binding");

        let event = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "group_added")
            .expect("group added event");
        let new_value: serde_json::Value =
            serde_json::from_str(event.new_value_json.as_deref().expect("new value"))
                .expect("new value json");

        assert_eq!(new_value["groupName"], "codex-pro");
        assert_eq!(new_value["defaultRateMultiplier"], 1.0);
        assert_eq!(new_value["userRateMultiplier"], 1.25);
        assert_eq!(new_value["effectiveRateMultiplier"], 1.25);
    }

    #[test]
    fn key_group_unresolved_event_is_emitted_for_missing_key_binding() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "unresolved-key-relay");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "unresolved key".to_string(),
                api_key: "sk-unresolved".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");

        database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: Some(key.id.clone()),
                binding_kind: BINDING_KIND_KEY_BINDING.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "key:unknown".to_string(),
                group_id_hash: None,
                group_name: "unknown".to_string(),
                binding_status: BINDING_STATUS_MISSING.to_string(),
                default_rate_multiplier: None,
                user_rate_multiplier: None,
                effective_rate_multiplier: None,
                rate_source: Some("key_probe".to_string()),
                confidence: 0.3,
                last_seen_at: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("missing binding");

        let events = database.list_change_events().expect("events");
        assert!(events.iter().any(|event| {
            event.event_type == "key_group_unresolved"
                && event.station_key_id.as_deref() == Some(key.id.as_str())
                && event.severity == "warning"
        }));
    }

    #[test]
    fn change_events_include_station_name_for_rows() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "station-name-row");

        database
            .upsert_change_event(crate::services::change_events::group_added_event(
                &station.id,
                "fast-group",
                "group-binding-station-name-row",
                Some(1.0),
                None,
                Some(1.0),
            ))
            .expect("insert event");

        let event = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "group_added")
            .expect("group event");

        assert_eq!(event.station_name.as_deref(), Some("station-name-row"));
    }

    #[test]
    fn group_rate_history_rate_changed_inserts_only_when_changed() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "group-rate-history");
        let binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "station:default".to_string(),
                group_id_hash: Some("default".to_string()),
                group_name: "default".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                rate_source: Some("test".to_string()),
                confidence: 0.9,
                last_seen_at: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("binding");

        let first = database
            .upsert_group_rate_record_if_changed(InsertGroupRateRecordInput {
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: Some(binding.id.clone()),
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                group_key_hash: "station:default".to_string(),
                group_name: "default".to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                source: "test".to_string(),
                confidence: 0.9,
                inferred_group_category: Some("unknown".to_string()),
                raw_json_redacted: None,
                checked_at: "1000".to_string(),
            })
            .expect("first");
        let duplicate = database
            .upsert_group_rate_record_if_changed(InsertGroupRateRecordInput {
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: Some(binding.id.clone()),
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                group_key_hash: "station:default".to_string(),
                group_name: "default".to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.0),
                effective_rate_multiplier: Some(1.0),
                source: "test".to_string(),
                confidence: 0.9,
                inferred_group_category: Some("unknown".to_string()),
                raw_json_redacted: None,
                checked_at: "2000".to_string(),
            })
            .expect("duplicate");
        let changed = database
            .upsert_group_rate_record_if_changed(InsertGroupRateRecordInput {
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: Some(binding.id),
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                group_key_hash: "station:default".to_string(),
                group_name: "default".to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: Some(1.4),
                effective_rate_multiplier: Some(1.4),
                source: "test".to_string(),
                confidence: 0.9,
                inferred_group_category: Some("unknown".to_string()),
                raw_json_redacted: None,
                checked_at: "3000".to_string(),
            })
            .expect("changed");

        let records = database
            .list_group_rate_records(station.id.clone())
            .expect("records");
        let events = database.list_change_events().expect("events");

        assert!(first.is_some());
        assert!(duplicate.is_none());
        assert!(changed.is_some());
        assert_eq!(records.len(), 2);
        assert!(events
            .iter()
            .any(|event| event.event_type == "rate_changed" && event.severity == "warning"));
    }

    #[test]
    fn legacy_key_group_name_migrates_to_key_binding_only() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "legacy-relay");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "legacy key".to_string(),
                api_key: "sk-legacy".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: Some("legacy-group".to_string()),
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");

        {
            let connection = database.connection().expect("connection");
            migrate_legacy_group_facts(&connection).expect("migrate once");
            migrate_legacy_group_facts(&connection).expect("migrate twice");
        }

        let bindings = database
            .list_station_group_bindings(station.id.clone())
            .expect("bindings");
        let key_bindings = bindings
            .iter()
            .filter(|binding| binding.station_key_id.as_deref() == Some(key.id.as_str()))
            .collect::<Vec<_>>();
        let legacy_station_groups = bindings
            .iter()
            .filter(|binding| {
                binding.binding_kind == "station_group"
                    && binding.rate_source.as_deref() == Some("legacy_key_group")
            })
            .collect::<Vec<_>>();

        assert_eq!(key_bindings.len(), 1);
        assert_eq!(key_bindings[0].binding_status, "manual_legacy");
        assert_eq!(legacy_station_groups.len(), 0);
    }

    #[test]
    fn full_collector_run_can_track_child_runs() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "run-relay");

        let parent = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "full".to_string(),
            })
            .expect("parent");
        let child = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: Some(parent.id.clone()),
                adapter: "sub2api".to_string(),
                task_type: "balance".to_string(),
            })
            .expect("child");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: child.id.clone(),
                status: "success".to_string(),
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                error_code: None,
                error_message: None,
                snapshot_id: None,
            })
            .expect("finish child");

        let runs = database.list_collector_runs(station.id).expect("runs");
        assert!(runs
            .iter()
            .any(|run| run.parent_run_id.as_deref() == Some(parent.id.as_str())));
    }

    #[test]
    fn change_events_table_is_initialized() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let connection = database.connection().expect("connection");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'change_events'",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(count, 1);
    }

    #[test]
    fn change_event_upsert_dedupes_and_can_be_resolved() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let first = database
            .upsert_change_event(UpsertChangeEventInput {
                severity: "warning".to_string(),
                event_type: "balance_low".to_string(),
                title: "余额偏低".to_string(),
                message: "测试站点余额低于阈值".to_string(),
                object_type: "station".to_string(),
                object_id: Some("station-1".to_string()),
                station_id: Some("station-1".to_string()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: Some("{\"value\":4.2}".to_string()),
                impact_json: None,
                dedupe_key: "balance:low:station:station-1".to_string(),
                source: "balance".to_string(),
            })
            .expect("first event");
        let second = database
            .upsert_change_event(UpsertChangeEventInput {
                severity: "warning".to_string(),
                event_type: "balance_low".to_string(),
                title: "余额偏低".to_string(),
                message: "测试站点余额仍低于阈值".to_string(),
                object_type: "station".to_string(),
                object_id: Some("station-1".to_string()),
                station_id: Some("station-1".to_string()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: Some("{\"value\":3.1}".to_string()),
                impact_json: None,
                dedupe_key: "balance:low:station:station-1".to_string(),
                source: "balance".to_string(),
            })
            .expect("second event");

        assert_eq!(first.id, second.id);
        assert_eq!(second.status, "unread");
        assert!(second.message.contains("仍低于"));

        let resolved = database
            .resolve_change_event(second.id.clone())
            .expect("resolved event");
        assert_eq!(resolved.status, "resolved");
        assert!(resolved.resolved_at.is_some());

        let events = database.list_change_events().expect("events");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn depleted_balance_event_stays_read_when_zero_balance_repeats() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "zero-balance-relay");

        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(0.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: Some(10.0),
                status: "depleted".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: Some("2026-07-09T01:00:00.000Z".to_string()),
            })
            .expect("first zero balance");

        let first_event = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "balance_depleted")
            .expect("depleted event");
        assert_eq!(first_event.status, "unread");

        std::thread::sleep(std::time::Duration::from_millis(2));

        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(0.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: Some(10.0),
                status: "depleted".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: Some("2026-07-09T01:03:00.000Z".to_string()),
            })
            .expect("repeated unread zero balance");

        let unread_repeat = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "balance_depleted")
            .expect("depleted event after unread repeat");
        assert_eq!(unread_repeat.status, "unread");
        assert_eq!(unread_repeat.detected_at, first_event.detected_at);
        assert_eq!(unread_repeat.updated_at, first_event.updated_at);

        let read_event = database
            .mark_change_event_read(first_event.id.clone())
            .expect("read event");

        std::thread::sleep(std::time::Duration::from_millis(2));

        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(0.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: Some(10.0),
                status: "depleted".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: Some("2026-07-09T01:05:00.000Z".to_string()),
            })
            .expect("repeated zero balance");

        let events = database.list_change_events().expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, first_event.id);
        assert_eq!(events[0].status, "read");
        assert_eq!(events[0].detected_at, first_event.detected_at);
        assert_eq!(events[0].updated_at, read_event.updated_at);
        assert!(events[0].message.contains('0'));
    }

    #[test]
    fn low_balance_snapshot_creates_change_event() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "low-balance-relay");

        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(4.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: Some(10.0),
                status: "low".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: None,
            })
            .expect("balance");

        let events = database.list_change_events().expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "balance_low");
        assert_eq!(events[0].severity, "warning");
        assert_eq!(events[0].station_id.as_deref(), Some(station.id.as_str()));
    }

    #[test]
    fn pricing_change_creates_warning_when_price_increases() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "price-relay");

        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-test".to_string(),
                input_price: Some(1.0),
                output_price: Some(2.0),
                fixed_price: None,
                rate_multiplier: None,
                currency: "USD".to_string(),
                unit: "1M tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: None,
                normalization_status: None,
                source: "test".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: None,
                valid_from: None,
                valid_until: None,
            })
            .expect("old price");
        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-test".to_string(),
                input_price: Some(1.0),
                output_price: Some(3.0),
                fixed_price: None,
                rate_multiplier: None,
                currency: "USD".to_string(),
                unit: "1M tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: None,
                normalization_status: None,
                source: "test".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: None,
                valid_from: None,
                valid_until: None,
            })
            .expect("new price");

        let events = database.list_change_events().expect("events");
        assert!(events
            .iter()
            .any(|event| event.event_type == "price_changed" && event.severity == "warning"));
    }

    #[test]
    fn collector_snapshot_rate_increase_creates_warning_event() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "rate-relay");

        database
            .insert_collector_snapshot(
                &station.id,
                "collector-test",
                "success",
                json!({ "ok": true }),
                json!({ "rateMultipliers": [{ "groupName": "default", "multiplier": 1.0 }] }),
                None,
                None,
            )
            .expect("first snapshot");
        database
            .insert_collector_snapshot(
                &station.id,
                "collector-test",
                "success",
                json!({ "ok": true }),
                json!({ "rateMultipliers": [{ "groupName": "default", "multiplier": 1.4 }] }),
                None,
                None,
            )
            .expect("second snapshot");

        let events = database.list_change_events().expect("events");
        assert!(events
            .iter()
            .any(|event| event.event_type == "rate_changed" && event.severity == "warning"));
    }

    #[test]
    fn newapi_model_collection_does_not_create_change_center_events() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "newapi-model-relay");
        set_station_type_for_test(&database, &station.id, "newapi");

        database
            .insert_collector_snapshot(
                &station.id,
                "newapi-models",
                "success",
                json!({ "ok": true }),
                json!({ "models": ["gpt-5.6-sol"] }),
                None,
                None,
            )
            .expect("first snapshot");
        database
            .insert_collector_snapshot(
                &station.id,
                "newapi-models",
                "success",
                json!({ "ok": true }),
                json!({ "models": ["gpt-5.6-sol", "grok-4.5"] }),
                None,
                None,
            )
            .expect("second snapshot");

        let events = database.list_change_events().expect("events");
        assert!(
            events
                .iter()
                .all(|event| event.event_type != "model_added"
                    && event.event_type != "model_removed"),
            "NewAPI model collection should stay out of the change center; got {events:?}",
        );
    }

    #[test]
    fn list_change_events_filters_existing_newapi_model_events() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "newapi-existing-events");
        set_station_type_for_test(&database, &station.id, "newapi");

        database
            .upsert_change_event(UpsertChangeEventInput {
                severity: crate::services::change_events::SEVERITY_INFO.to_string(),
                event_type: "model_added".to_string(),
                title: "模型新增".to_string(),
                message: "站点新增模型 grok-4.5".to_string(),
                object_type: "station".to_string(),
                object_id: Some(station.id.clone()),
                station_id: Some(station.id.clone()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: Some(json!({ "model": "grok-4.5" }).to_string()),
                impact_json: None,
                dedupe_key: "old-newapi-model-added".to_string(),
                source: "collector".to_string(),
            })
            .expect("model event");
        database
            .upsert_change_event(UpsertChangeEventInput {
                severity: crate::services::change_events::SEVERITY_WARNING.to_string(),
                event_type: "low_balance".to_string(),
                title: "余额偏低".to_string(),
                message: "余额低于阈值".to_string(),
                object_type: "station".to_string(),
                object_id: Some(station.id.clone()),
                station_id: Some(station.id.clone()),
                station_key_id: None,
                pricing_rule_id: None,
                request_log_id: None,
                old_value_json: None,
                new_value_json: None,
                impact_json: None,
                dedupe_key: "newapi-low-balance".to_string(),
                source: "balance".to_string(),
            })
            .expect("balance event");

        let events = database.list_change_events().expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "low_balance");

        let station_events = database
            .list_change_events_for_station(station.id)
            .expect("station events");
        assert_eq!(station_events.len(), 1);
        assert_eq!(station_events[0].event_type, "low_balance");
    }

    #[test]
    fn station_key_capabilities_round_trip() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "routing-capabilities");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        let input = UpdateStationKeyCapabilitiesInput {
            station_key_id: key.id.clone(),
            supports_chat_completions: true,
            supports_responses: false,
            supports_embeddings: true,
            supports_stream: true,
            supports_tools: true,
            supports_vision: false,
            supports_reasoning: true,
            model_allowlist: vec!["gpt-4o-mini".to_string()],
            model_blocklist: vec!["gpt-4o".to_string()],
            preferred_models: vec!["gpt-4o-mini".to_string()],
            only_use_as_backup: false,
            routing_tags: vec!["cheap".to_string()],
        };

        let saved = database
            .update_station_key_capabilities(input)
            .expect("save");
        let loaded = database.get_station_key_capabilities(key.id).expect("load");

        assert_eq!(loaded.station_key_id, saved.station_key_id);
        assert_eq!(loaded.model_allowlist, vec!["gpt-4o-mini"]);
        assert_eq!(loaded.model_blocklist, vec!["gpt-4o"]);
        assert!(loaded.supports_tools);
        assert!(loaded.supports_reasoning);
    }

    #[test]
    fn local_routing_order_migration_initializes_global_order_without_changing_priority() {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                "
                CREATE TABLE stations (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    station_type TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    upstream_api_format TEXT NOT NULL DEFAULT 'auto',
                    upstream_api_base_path TEXT NOT NULL DEFAULT '/v1',
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    credit_per_cny REAL NOT NULL DEFAULT 1,
                    balance_raw REAL,
                    balance_cny REAL,
                    low_balance_threshold_cny REAL,
                    collection_interval_minutes INTEGER NOT NULL DEFAULT 5,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    latency_ms INTEGER,
                    last_checked_at TEXT,
                    last_pricing_fetched_at TEXT,
                    note TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE station_keys (
                    id TEXT PRIMARY KEY,
                    station_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 0,
                    group_name TEXT,
                    tier_label TEXT,
                    status TEXT NOT NULL DEFAULT 'unchecked',
                    last_checked_at TEXT,
                    last_used_at TEXT,
                    note TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
                );

                INSERT INTO stations (
                    id, name, station_type, base_url, api_key, created_at, updated_at
                ) VALUES
                    ('station-a', 'Station A', 'openai-compatible', 'https://a.test', 'sk-a', '1000', '1000'),
                    ('station-b', 'Station B', 'openai-compatible', 'https://b.test', 'sk-b', '1000', '1000');

                INSERT INTO station_keys (
                    id, station_id, name, api_key, enabled, priority, created_at, updated_at
                ) VALUES
                    ('key-third', 'station-a', 'third', 'sk-third', 1, 2, '1000', '1000'),
                    ('key-second', 'station-b', 'second', 'sk-second', 1, 1, '1000', '1000'),
                    ('key-first', 'station-a', 'first', 'sk-first', 1, 1, '0500', '0500'),
                    ('key-fourth', 'station-b', 'fourth', 'sk-fourth', 1, 2, '1000', '1000');
                ",
            )
            .expect("legacy schema");

        initialize_schema(&connection).expect("migrate schema");

        let rows = connection
            .prepare(
                "SELECT id, priority, routing_order
                   FROM station_keys
                  ORDER BY routing_order ASC",
            )
            .expect("select station keys")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .expect("query station keys")
            .collect::<Result<Vec<_>, _>>()
            .expect("station key rows");

        assert_eq!(
            rows,
            vec![
                ("key-first".to_string(), 1, 0),
                ("key-second".to_string(), 1, 1),
                ("key-fourth".to_string(), 2, 2),
                ("key-third".to_string(), 2, 3),
            ]
        );
    }

    #[test]
    fn key_pool_items_follow_global_routing_order_when_priorities_repeat() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let first_station = test_station(&database, "first-key-pool-route");
        let second_station = test_station(&database, "second-key-pool-route");
        let first_key = database
            .list_station_keys(first_station.id)
            .expect("first station keys")
            .remove(0);
        let second_key = database
            .list_station_keys(second_station.id)
            .expect("second station keys")
            .remove(0);

        assert_eq!(first_key.priority, 0);
        assert_eq!(second_key.priority, 0);

        database
            .reorder_local_routing_keys(vec![second_key.id.clone(), first_key.id.clone()])
            .expect("persist global routing order");

        let items = database.list_key_pool_items().expect("key pool items");
        let ids = items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        let priorities = items.iter().map(|item| item.priority).collect::<Vec<_>>();

        assert_eq!(ids, vec![second_key.id.as_str(), first_key.id.as_str()]);
        assert_eq!(priorities, vec![0, 0]);
    }

    #[test]
    fn new_station_key_appends_after_persisted_local_routing_order() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let first_station = test_station(&database, "first-route");
        let second_station = test_station(&database, "second-route");
        let first_key = database
            .list_station_keys(first_station.id.clone())
            .expect("first keys")
            .remove(0);
        let second_key = database
            .list_station_keys(second_station.id)
            .expect("second keys")
            .remove(0);

        database
            .reorder_local_routing_keys(vec![second_key.id.clone(), first_key.id.clone()])
            .expect("persist routing order");
        let new_key = database
            .create_station_key(CreateStationKeyInput {
                station_id: first_station.id,
                name: "New appended key".to_string(),
                api_key: "sk-new-appended".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("new key");

        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace");
        let ids = workspace
            .candidates
            .iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                second_key.id.as_str(),
                first_key.id.as_str(),
                new_key.id.as_str()
            ]
        );
    }

    #[test]
    fn model_alias_round_trip() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let saved = database
            .upsert_model_alias(UpsertModelAliasInput {
                id: None,
                client_model: "gpt-4o-mini".to_string(),
                upstream_model: "openai/gpt-4o-mini".to_string(),
                enabled: true,
                note: Some("test alias".to_string()),
            })
            .expect("save alias");

        let aliases = database.list_model_aliases().expect("aliases");

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].id, saved.id);
        assert_eq!(aliases[0].client_model, "gpt-4o-mini");
        assert_eq!(aliases[0].upstream_model, "openai/gpt-4o-mini");
    }

    #[test]
    fn successful_request_updates_key_health_success() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "success-key");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        database
            .record_station_key_success(&key.id, 123, "1000")
            .expect("success");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.avg_latency_ms, Some(123));
        assert_eq!(health.last_success_at.as_deref(), Some("1000"));
        assert_eq!(health.cooldown_until, None);
    }

    #[test]
    fn station_key_health_success_resets_prior_revision_failure_state() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "revision-success-key");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .record_station_key_failure(&key.id, "old endpoint timeout", "1000")
            .expect("old endpoint failure");
        let updated_station = change_test_station_endpoint(&database, &station);
        assert_eq!(
            updated_station.endpoint_revision,
            station.endpoint_revision + 1
        );

        database
            .record_station_key_success(&key.id, 45, "2000")
            .expect("new endpoint success");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(health.last_success_at.as_deref(), Some("2000"));
        assert_eq!(health.last_failure_at, None);
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.avg_latency_ms, Some(45));
        assert_eq!(health.last_error_summary, None);
        assert_eq!(health.cooldown_until, None);
    }

    #[test]
    fn station_key_health_failure_resets_prior_revision_success_state() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "revision-failure-key");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .record_station_key_success(&key.id, 123, "1000")
            .expect("old endpoint success");
        let updated_station = change_test_station_endpoint(&database, &station);
        assert_eq!(
            updated_station.endpoint_revision,
            station.endpoint_revision + 1
        );

        database
            .record_station_key_failure(&key.id, "new endpoint timeout", "2000")
            .expect("new endpoint failure");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(health.last_success_at, None);
        assert_eq!(health.last_failure_at.as_deref(), Some("2000"));
        assert_eq!(health.success_count, 0);
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.avg_latency_ms, None);
        assert_eq!(
            health.last_error_summary.as_deref(),
            Some("new endpoint timeout")
        );
        assert_eq!(health.cooldown_until, None);
    }

    #[test]
    fn repeated_failures_enter_cooldown() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "failure-key");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        database
            .record_station_key_failure(&key.id, "timeout", "1000")
            .expect("failure 1");
        database
            .record_station_key_failure(&key.id, "timeout", "2000")
            .expect("failure 2");
        database
            .record_station_key_failure(&key.id, "timeout", "3000")
            .expect("failure 3");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(health.failure_count, 3);
        assert_eq!(health.consecutive_failures, 3);
        assert_eq!(health.last_error_summary.as_deref(), Some("timeout"));
        assert_eq!(health.cooldown_until.as_deref(), Some("123000"));

        let events = database.list_change_events().expect("events");
        let key_event = events
            .iter()
            .find(|event| event.event_type == "key_invalid")
            .expect("key health event");
        let payload: serde_json::Value =
            serde_json::from_str(key_event.new_value_json.as_deref().expect("payload"))
                .expect("valid key health payload");

        assert_eq!(payload["stationKeyName"], key.name);
        assert_eq!(payload["apiKeyMasked"], key.api_key_masked);
    }

    #[test]
    fn explicit_failure_cooldown_is_persisted_without_threshold_policy() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "explicit-cooldown-key");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        database
            .record_station_key_failure_with_cooldown(
                &key.id,
                "rate limited",
                "1000",
                Some("91000"),
            )
            .expect("failure with explicit cooldown");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.last_error_summary.as_deref(), Some("rate limited"));
        assert_eq!(health.cooldown_until.as_deref(), Some("91000"));
    }

    #[test]
    fn local_routing_workspace_shows_hard_failure_as_offline() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "offline-key");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        database
            .record_station_key_failure_with_cooldown(
                &key.id,
                "auth_error: upstream returned HTTP 401",
                "1000",
                Some("901000"),
            )
            .expect("hard failure");
        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace");
        let candidate = workspace
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == key.id)
            .expect("candidate");

        assert_eq!(
            candidate.health_state,
            crate::services::proxy::routing_types::RouteHealthState::Offline
        );
    }

    #[test]
    fn interrupted_request_log_is_not_reported_as_selected_route() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "interrupted-route");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4".to_string()),
                stream: true,
                status: "interrupted".to_string(),
                lifecycle_status: Some("interrupted".to_string()),
                station_key_id: Some(key.id.clone()),
                station_id: Some(station.id.clone()),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: Some("client disconnected before stream completed".to_string()),
                route_policy: Some("priority_fallback".to_string()),
                route_reason: None,
                rejected_candidates_json: Some("[]".to_string()),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                started_at: "2000".to_string(),
                finished_at: Some("2500".to_string()),
                duration_ms: Some(500),
            })
            .expect("interrupted log");

        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace");
        let decision_id = workspace.recent_events[0].decision_id.clone();
        let events = database.list_change_events().expect("events");

        assert_eq!(
            workspace.latest_decision.expect("latest decision").status,
            crate::services::proxy::routing_types::RouteDecisionStatus::Failed
        );
        assert_eq!(workspace.recent_events[0].accepted, false);
        assert!(workspace.recent_events[0].message.contains("interrupted"));
        assert!(events.iter().any(|event| {
            event.event_type == "route_impacted"
                && event.request_log_id.as_deref() == Some(decision_id.as_str())
        }));
    }

    #[test]
    fn simulate_route_returns_selected_key_and_rejection_reasons() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let selected_station = test_station(&database, "selected-route-key");
        let blocked_station = test_station(&database, "blocked-route-key");
        let selected = database
            .list_station_keys(selected_station.id)
            .expect("selected keys")
            .remove(0);
        let blocked = database
            .list_station_keys(blocked_station.id)
            .expect("blocked keys")
            .remove(0);

        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: selected.id.clone(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: false,
                supports_stream: true,
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                model_allowlist: vec!["gpt-5.4".to_string()],
                model_blocklist: Vec::new(),
                preferred_models: vec!["gpt-5.4".to_string()],
                only_use_as_backup: false,
                routing_tags: Vec::new(),
            })
            .expect("selected caps");
        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: blocked.id.clone(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: false,
                supports_stream: true,
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                model_allowlist: vec!["gpt-4o-mini".to_string()],
                model_blocklist: Vec::new(),
                preferred_models: Vec::new(),
                only_use_as_backup: false,
                routing_tags: Vec::new(),
            })
            .expect("blocked caps");

        let result = database
            .simulate_route(RouteSimulationInput {
                endpoint: RouteEndpointKind::ChatCompletions,
                model: Some("gpt-5.4".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                policy: Some(RoutingPolicy::PriorityFallback),
                max_rate_multiplier: None,
                routing_group_filter: None,
                session_hash: None,
                previous_response_id: None,
            })
            .expect("simulate");

        assert_eq!(
            result.selected_station_key_id.as_deref(),
            Some(selected.id.as_str())
        );
        assert_eq!(
            result.selected_station_id.as_deref(),
            Some(selected.station_id.as_str())
        );
        assert!(result.candidates.iter().any(|candidate| {
            candidate.station_key_id == blocked.id
                && !candidate.accepted
                && candidate
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason.contains("allowlist"))
        }));
    }

    #[test]
    fn automatic_simulate_route_returns_structured_rejection_when_over_multiplier_limit() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "automatic-over-limit");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        database
            .update_station_key(UpdateStationKeyInput {
                id: key.id.clone(),
                station_id: key.station_id.clone(),
                name: key.name.clone(),
                api_key: None,
                enabled: true,
                priority: key.priority,
                max_concurrency: key.max_concurrency,
                load_factor: key.load_factor,
                schedulable: true,
                group_name: key.group_name.clone(),
                tier_label: key.tier_label.clone(),
                group_binding_id: key.group_binding_id.clone(),
                group_id_hash: key.group_id_hash.clone(),
                rate_multiplier: key.rate_multiplier,
                manual_rate_multiplier: Some(Some(2.0)),
                rate_source: key.rate_source.clone(),
                balance_scope: key.balance_scope.clone(),
                status: key.status.clone(),
                note: key.note.clone(),
            })
            .expect("manual multiplier");

        let result = database
            .simulate_route(RouteSimulationInput {
                endpoint: RouteEndpointKind::ChatCompletions,
                model: Some("gpt-5.4".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                policy: Some(RoutingPolicy::AutomaticBalanced),
                max_rate_multiplier: Some(1.0),
                routing_group_filter: None,
                session_hash: None,
                previous_response_id: None,
            })
            .expect("automatic hard rejection should still return explanation");

        assert_eq!(result.selected_station_key_id, None);
        assert_eq!(
            result.scheduler_error_code.as_deref(),
            Some("routing_no_candidate_within_multiplier_limit")
        );
        assert!(result.candidates.iter().any(|candidate| {
            candidate.station_key_id == key.id
                && !candidate.accepted
                && candidate.rate_multiplier == Some(2.0)
                && candidate
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason == "multiplier_over_ceiling")
        }));
    }

    #[test]
    fn simulate_route_rejects_key_after_hard_failure() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let offline_station = test_station(&database, "offline-route-key");
        let ready_station = test_station(&database, "ready-route-key");
        let offline = database
            .list_station_keys(offline_station.id)
            .expect("offline keys")
            .remove(0);
        let ready = database
            .list_station_keys(ready_station.id)
            .expect("ready keys")
            .remove(0);
        database
            .reorder_key_pool(vec![offline.id.clone(), ready.id.clone()])
            .expect("priority order");
        database
            .record_station_key_failure_with_cooldown(
                &offline.id,
                "auth_error: upstream returned HTTP 401",
                "1000",
                Some("901000"),
            )
            .expect("hard failure");

        let result = database
            .simulate_route(RouteSimulationInput {
                endpoint: RouteEndpointKind::ChatCompletions,
                model: Some("gpt-5.4".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                policy: Some(RoutingPolicy::PriorityFallback),
                max_rate_multiplier: None,
                routing_group_filter: None,
                session_hash: None,
                previous_response_id: None,
            })
            .expect("simulate");

        assert_eq!(
            result.selected_station_key_id.as_deref(),
            Some(ready.id.as_str())
        );
        assert!(result.candidates.iter().any(|candidate| {
            candidate.station_key_id == offline.id
                && !candidate.accepted
                && candidate
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason.contains("offline"))
        }));
    }

    #[test]
    fn request_log_records_route_policy_and_reason_without_prompt() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4".to_string()),
                stream: false,
                status: "success".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some("key-1".to_string()),
                station_id: Some("station-1".to_string()),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: None,
                route_policy: Some("priority_fallback".to_string()),
                route_reason: Some("selected key-1 because model allowed".to_string()),
                rejected_candidates_json: Some("[]".to_string()),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                base_input_cost: Some(0.04),
                base_output_cost: Some(0.08),
                base_fixed_cost: None,
                base_total_cost: Some(0.12),
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
                group_binding_id: Some("group-1".to_string()),
                normalization_status: Some("complete".to_string()),
                balance_scope: Some("station".to_string()),
                economic_context_json: Some(
                    serde_json::json!({
                        "groupBindingId": "group-1",
                        "normalizationStatus": "complete",
                        "balanceScope": "station"
                    })
                    .to_string(),
                ),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        assert_eq!(log.route_policy.as_deref(), Some("priority_fallback"));
        assert_eq!(log.lifecycle_status.as_deref(), Some("completed"));
        assert_eq!(
            log.route_reason.as_deref(),
            Some("selected key-1 because model allowed")
        );
        assert_eq!(log.rejected_candidates_json.as_deref(), Some("[]"));
        assert_eq!(log.group_binding_id.as_deref(), Some("group-1"));
        assert_eq!(log.normalization_status.as_deref(), Some("complete"));
        assert_eq!(log.balance_scope.as_deref(), Some("station"));
        assert_option_f64_close(log.base_total_cost, 0.12);
        let serialized = serde_json::to_string(&log).unwrap();
        assert!(!serialized.contains("\"prompt\":"));
    }

    #[test]
    fn list_request_logs_preserves_usage_only_snapshot_without_repricing() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "request-log-usage-only-snapshot");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4-mini".to_string()),
                stream: false,
                status: "success".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some(key.id),
                station_id: Some(station.id),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: None,
                route_policy: Some("priority_fallback".to_string()),
                route_reason: None,
                rejected_candidates_json: None,
                prompt_tokens: Some(10),
                completion_tokens: Some(11),
                total_tokens: Some(21),
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: Some("usage_only".to_string()),
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        let listed = database.list_request_logs().expect("request logs");
        let listed_log = listed
            .iter()
            .find(|candidate| candidate.id == log.id)
            .expect("listed log");

        assert_eq!(listed_log.estimated_input_cost, None);
        assert_eq!(listed_log.estimated_output_cost, None);
        assert_eq!(listed_log.estimated_total_cost, None);
        assert_eq!(listed_log.cost_currency, None);
        assert_eq!(listed_log.pricing_source, None);
        assert_eq!(listed_log.cost_status.as_deref(), Some("usage_only"));
    }

    #[test]
    fn list_local_proxy_request_logs_excludes_channel_monitor_rows() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "INSERT INTO request_logs
                     (id, started_at, method, path, stream, status, route_policy, created_at)
                     VALUES (?1, ?2, 'POST', '/v1/responses', 0, 'success', ?3, ?2)",
                    params!["monitor-row", "1000", "channel_monitor"],
                )
                .expect("monitor row");
            connection
                .execute(
                    "INSERT INTO request_logs
                     (id, started_at, method, path, stream, status, route_policy, created_at)
                     VALUES (?1, ?2, 'POST', '/v1/responses', 0, 'success', ?3, ?2)",
                    params!["proxy-row", "2000", "cost_stable_first"],
                )
                .expect("proxy row");
        }

        let logs = database
            .list_local_proxy_request_logs()
            .expect("local proxy logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].id, "proxy-row");
    }

    #[test]
    fn list_request_logs_marks_legacy_backfilled_costs() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "request-log-base-price");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4-mini".to_string()),
                stream: false,
                status: "success".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some(key.id),
                station_id: Some(station.id),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: None,
                route_policy: Some("priority_fallback".to_string()),
                route_reason: None,
                rejected_candidates_json: None,
                prompt_tokens: Some(10),
                completion_tokens: Some(11),
                total_tokens: Some(21),
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        let listed = database.list_request_logs().expect("request logs");
        let listed_log = listed
            .iter()
            .find(|candidate| candidate.id == log.id)
            .expect("listed log");

        assert_option_f64_close(listed_log.estimated_input_cost, 0.0000075);
        assert_option_f64_close(listed_log.estimated_output_cost, 0.0000495);
        assert_option_f64_close(listed_log.estimated_total_cost, 0.000057);
        assert_option_f64_close(listed_log.base_total_cost, 0.000057);
        assert_eq!(listed_log.cost_currency.as_deref(), Some("USD"));
        assert_eq!(
            listed_log.pricing_source.as_deref(),
            Some("model_base_price")
        );
        assert_eq!(listed_log.cost_status.as_deref(), Some("legacy_estimate"));
    }

    #[test]
    fn request_log_round_trips_observability_fields() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/responses".to_string(),
                model: Some("gpt-5.5".to_string()),
                stream: true,
                status: "success".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some("key-observed".to_string()),
                station_id: Some("station-observed".to_string()),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: None,
                route_policy: Some("cost_stable_first".to_string()),
                route_reason: None,
                rejected_candidates_json: None,
                prompt_tokens: Some(101),
                completion_tokens: Some(23),
                total_tokens: Some(124),
                cache_creation_tokens: Some(17),
                cache_read_tokens: Some(83),
                reasoning_effort: Some("high".to_string()),
                first_token_ms: Some(321),
                billing_mode: Some("token".to_string()),
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: Some(0.0042),
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: Some("USD".to_string()),
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: Some("priced".to_string()),
                group_binding_id: Some("group-observed".to_string()),
                normalization_status: Some("complete".to_string()),
                balance_scope: Some("key".to_string()),
                economic_context_json: None,
                started_at: "1000".to_string(),
                finished_at: Some("2000".to_string()),
                duration_ms: Some(1000),
            })
            .expect("insert observed request log");

        let listed = database.list_request_logs().expect("request logs");
        let listed = listed
            .iter()
            .find(|candidate| candidate.id == log.id)
            .expect("listed observed request log");

        assert_eq!(listed.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(listed.cache_creation_tokens, Some(17));
        assert_eq!(listed.cache_read_tokens, Some(83));
        assert_eq!(listed.first_token_ms, Some(321));
        assert_eq!(listed.billing_mode.as_deref(), Some("token"));
    }

    #[test]
    fn list_request_logs_backfills_missing_base_cost_without_repricing_snapshot() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "request-log-existing-cost-missing-base");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4-mini".to_string()),
                stream: false,
                status: "success".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some(key.id),
                station_id: Some(station.id),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 0,
                error_message: None,
                route_policy: Some("priority_fallback".to_string()),
                route_reason: None,
                rejected_candidates_json: None,
                prompt_tokens: Some(10),
                completion_tokens: Some(11),
                total_tokens: Some(21),
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: Some(0.5),
                estimated_output_cost: Some(0.25),
                estimated_total_cost: Some(0.75),
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: Some("USD".to_string()),
                pricing_rule_id: None,
                pricing_source: Some("model_base_price".to_string()),
                cost_status: Some("base_price_only".to_string()),
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        let listed = database.list_request_logs().expect("request logs");
        let listed_log = listed
            .iter()
            .find(|candidate| candidate.id == log.id)
            .expect("listed log");

        assert_option_f64_close(listed_log.estimated_total_cost, 0.75);
        assert_option_f64_close(listed_log.base_total_cost, 0.000057);
        assert_eq!(listed_log.cost_status.as_deref(), Some("base_price_only"));
    }

    #[test]
    fn request_log_redacts_error() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4".to_string()),
                stream: false,
                status: "failed".to_string(),
                lifecycle_status: Some("completed".to_string()),
                station_key_id: Some("key-1".to_string()),
                station_id: Some("station-1".to_string()),
                upstream_base_url: Some("https://example.test".to_string()),
                fallback_count: 1,
                error_message: Some(
                    "upstream rejected Authorization: Bearer sk-p8-secret-plaintext-canary"
                        .to_string(),
                ),
                route_policy: Some("priority_fallback".to_string()),
                route_reason: Some(
                    "selected key after token=p8-token-canary was rejected".to_string(),
                ),
                rejected_candidates_json: Some(
                    serde_json::json!({
                        "api_key": "sk-p8-secret-plaintext-canary",
                        "reason": "cookie rpd_session=p8-cookie-canary failed"
                    })
                    .to_string(),
                ),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                cache_creation_tokens: None,
                cache_read_tokens: None,
                reasoning_effort: None,
                first_token_ms: None,
                billing_mode: None,
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                base_input_cost: None,
                base_output_cost: None,
                base_fixed_cost: None,
                base_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: Some(
                    serde_json::json!({
                        "authorization": "Bearer sk-p8-secret-plaintext-canary",
                        "cookie": "rpd_session=p8-cookie-canary"
                    })
                    .to_string(),
                ),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        let serialized = serde_json::to_string(&log).expect("json");
        assert!(serialized.contains("[REDACTED]"));
        assert!(!serialized.contains("sk-p8-secret-plaintext-canary"));
        assert!(!serialized.contains("p8-token-canary"));
        assert!(!serialized.contains("rpd_session=p8-cookie-canary"));
    }

    #[test]
    fn collector_snapshot_redacts_raw_secret_fields() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "snapshot-redaction");
        let snapshot = database
            .insert_collector_snapshot(
                &station.id,
                "collector-test",
                "failed",
                serde_json::json!({
                    "password": "p8-password-canary",
                    "balance": 1
                }),
                serde_json::json!({
                    "headers": {
                        "authorization": "Bearer sk-p8-secret-plaintext-canary"
                    }
                }),
                Some(serde_json::json!({
                    "cookie": "rpd_session=p8-cookie-canary",
                    "items": [
                        { "api_key": "sk-p8-secret-plaintext-canary" }
                    ]
                })),
                Some("failed with token=p8-token-canary".to_string()),
            )
            .expect("snapshot");

        let serialized = serde_json::to_string(&snapshot).expect("json");
        assert!(serialized.contains("[REDACTED]"));
        assert!(!serialized.contains("sk-p8-secret-plaintext-canary"));
        assert!(!serialized.contains("p8-password-canary"));
        assert!(!serialized.contains("rpd_session=p8-cookie-canary"));
        assert!(!serialized.contains("p8-token-canary"));
    }

    #[test]
    fn secret_safety_scan_finds_plaintext_canary() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "scan-canary");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "scan canary".to_string(),
                api_key: "sk-not-the-canary".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");
        {
            let connection = database.connection().expect("connection");
            connection
                .execute(
                    "UPDATE station_keys SET api_key = ?1 WHERE id = ?2",
                    params!["sk-p8-secret-plaintext-canary", key.id],
                )
                .expect("write canary");
        }

        let findings = database.run_secret_safety_scan().expect("scan");

        assert!(findings.iter().any(|finding| {
            finding.table_name == "station_keys" && finding.column_name == "api_key"
        }));
        assert!(findings
            .iter()
            .all(|finding| !finding.evidence.contains("canary")));
    }

    #[test]
    fn migrating_plain_station_key_moves_secret_out_of_plain_column() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "secret-migration");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "canary key".to_string(),
                api_key: "sk-p8-secret-plaintext-canary".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");
        let data_key = crate::services::secrets::crypto::generate_data_key();

        let report = database
            .migrate_plaintext_secrets_for_tests(&data_key)
            .expect("migrate");

        assert!(report.migrated_count >= 1);
        assert_eq!(report.failed_count, 0);
        let (plain, secret_id): (String, Option<String>) = {
            let connection = database.connection().expect("connection");
            connection
                .query_row(
                    "SELECT api_key, api_key_secret_id FROM station_keys WHERE id = ?1",
                    params![key.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("row")
        };
        assert_eq!(plain, "");
        assert!(secret_id.is_some());

        let secret_id = secret_id.expect("secret id");
        let (ciphertext, masked): (String, String) = {
            let connection = database.connection().expect("connection");
            connection
                .query_row(
                    "SELECT ciphertext, masked_value FROM secrets WHERE id = ?1",
                    params![secret_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("secret")
        };
        assert!(!ciphertext.contains("sk-p8-secret-plaintext-canary"));
        assert_eq!(masked, "sk-...nary");
    }

    #[test]
    fn migrated_secret_can_be_decrypted_for_routing() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "secret-route");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "route key".to_string(),
                api_key: "sk-p8-secret-plaintext-canary".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        database
            .migrate_plaintext_secrets_for_tests(&data_key)
            .expect("migrate");

        let decrypted = database
            .resolve_station_key_secret_for_tests(&data_key, &key.id)
            .expect("decrypt");
        let candidates = database
            .proxy_route_candidates_with_data_key_for_tests(&data_key)
            .expect("candidates");

        assert_eq!(decrypted, "sk-p8-secret-plaintext-canary");
        assert!(candidates.iter().any(|candidate| {
            candidate.station_key_id == key.id
                && candidate.api_key == "sk-p8-secret-plaintext-canary"
        }));
    }

    #[test]
    fn local_routing_workspace_loads_migrated_secret_key_without_data_key() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "secret-read-model");
        let key = database
            .create_station_key(CreateStationKeyInput {
                station_id: station.id,
                name: "read model key".to_string(),
                api_key: "sk-p8-secret-plaintext-canary".to_string(),
                enabled: true,
                priority: Some(0),
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .expect("key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        database
            .migrate_plaintext_secrets_for_tests(&data_key)
            .expect("migrate");

        let workspace = database
            .load_local_routing_workspace(crate::models::proxy::ProxyStatus {
                running: false,
                lifecycle: ProxyLifecycle::Stopped,
                bind_addr: "127.0.0.1".to_string(),
                port: 8787,
                started_at: None,
                last_error: None,
                active_requests: 0,
                request_count: 0,
            })
            .expect("workspace without data key");

        assert!(workspace
            .candidates
            .iter()
            .any(|candidate| candidate.station_key_id == key.id));
    }

    #[test]
    fn encrypted_station_key_write_keeps_plain_column_empty() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "encrypted-key-write");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id,
                    name: "encrypted key".to_string(),
                    api_key: "sk-p8-secret-plaintext-canary".to_string(),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: None,
                    load_factor: None,
                    schedulable: None,
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    note: None,
                },
                &data_key,
            )
            .expect("key");

        let (plain, secret_id): (String, Option<String>) = {
            let connection = database.connection().expect("connection");
            connection
                .query_row(
                    "SELECT api_key, api_key_secret_id FROM station_keys WHERE id = ?1",
                    params![key.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("row")
        };
        let decrypted = database
            .resolve_station_key_secret_for_tests(&data_key, &key.id)
            .expect("decrypt");

        assert_eq!(plain, "");
        assert!(secret_id.is_some());
        assert_eq!(key.api_key_masked, "sk-...nary");
        assert_eq!(decrypted, "sk-p8-secret-plaintext-canary");
    }

    #[test]
    fn encrypted_station_key_blank_update_preserves_secret() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "encrypted-key-update");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: "encrypted key".to_string(),
                    api_key: "sk-p8-secret-plaintext-canary".to_string(),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: None,
                    load_factor: None,
                    schedulable: None,
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    note: None,
                },
                &data_key,
            )
            .expect("key");

        let updated = database
            .update_station_key_with_data_key(
                UpdateStationKeyInput {
                    id: key.id.clone(),
                    station_id: station.id,
                    name: "renamed encrypted key".to_string(),
                    api_key: Some("   ".to_string()),
                    enabled: true,
                    priority: key.priority,
                    max_concurrency: 3,
                    load_factor: None,
                    schedulable: true,
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    status: key.status,
                    note: None,
                },
                &data_key,
            )
            .expect("update");
        let decrypted = database
            .resolve_station_key_secret_for_tests(&data_key, &updated.id)
            .expect("decrypt");

        assert_eq!(updated.name, "renamed encrypted key");
        assert_eq!(decrypted, "sk-p8-secret-plaintext-canary");
    }

    #[test]
    fn encrypted_station_credentials_write_keeps_plain_password_empty() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "encrypted-credentials");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let credentials = database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("p8-password-canary".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let (plain, secret_id): (Option<String>, Option<String>) = {
            let connection = database.connection().expect("connection");
            connection
                .query_row(
                    "SELECT login_password, login_password_secret_id FROM station_credentials WHERE station_id = ?1",
                    params![station.id.clone()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("row")
        };
        let decrypted = database
            .get_station_login_password_with_data_key(station.id, &data_key)
            .expect("password");

        assert!(credentials.password_present);
        assert!(plain.is_none());
        assert!(secret_id.is_some());
        assert_eq!(decrypted.as_deref(), Some("p8-password-canary"));
    }

    #[test]
    fn manual_station_session_write_keeps_plain_tokens_out_of_credentials() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "manual-session");
        let data_key = crate::services::secrets::crypto::generate_data_key();

        let credentials = database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("p9-access-token-canary".to_string()),
                    refresh_token: Some("p9-refresh-token-canary".to_string()),
                    cookie: Some("rpd_session=p9-cookie-canary".to_string()),
                    newapi_user_id: Some("user-123".to_string()),
                    token_expires_at: Some("200000".to_string()),
                },
                &data_key,
            )
            .expect("session");

        let (access_secret_id, refresh_secret_id, cookie_secret_id): (
            Option<String>,
            Option<String>,
            Option<String>,
        ) = {
            let connection = database.connection().expect("connection");
            connection
                .query_row(
                    "SELECT access_token_secret_id, refresh_token_secret_id, cookie_secret_id
                       FROM station_credentials
                      WHERE station_id = ?1",
                    params![station.id.clone()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("row")
        };
        let resolved = database
            .resolve_station_session_with_data_key(station.id, &data_key, 100000)
            .expect("resolved session");

        assert!(credentials.access_token_present);
        assert!(credentials.refresh_token_present);
        assert!(credentials.cookie_present);
        assert_eq!(credentials.newapi_user_id.as_deref(), Some("user-123"));
        assert_eq!(credentials.session_source, "manual_token");
        assert!(access_secret_id.is_some());
        assert!(refresh_secret_id.is_some());
        assert!(cookie_secret_id.is_some());
        assert_eq!(resolved.status, SessionResolveStatus::Ready);
        assert_eq!(
            resolved.access_token.as_deref(),
            Some("p9-access-token-canary")
        );
    }

    #[test]
    fn newapi_session_cookie_only_is_ready() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "newapi-cookie-only");
        let data_key = [37_u8; 32];

        database
            .persist_station_session_with_data_key(
                PersistStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some("session=encrypted-at-rest".to_string()),
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: "password_login".to_string(),
                },
                &data_key,
            )
            .expect("persist session");
        let station_id = station.id.clone();
        let session = database
            .resolve_station_session_with_data_key(station.id, &data_key, 100_000)
            .expect("resolve session");
        let credentials = database
            .get_station_credentials(station_id)
            .expect("credentials");

        assert_eq!(session.status, SessionResolveStatus::Ready);
        assert_eq!(session.cookie.as_deref(), Some("session=encrypted-at-rest"));
        assert_eq!(session.newapi_user_id.as_deref(), Some("42"));
        assert_eq!(credentials.session_source, "password_login");
    }

    #[test]
    fn stale_endpoint_revision_rejects_session_persistence() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "stale-session");
        let data_key = [43_u8; 32];
        let old_revision = station.endpoint_revision;
        let updated = update_test_station_urls(
            &database,
            &station,
            "https://new-console.example".to_string(),
            station.api_base_url.clone(),
            true,
        );
        assert!(updated.endpoint_revision > old_revision);

        let error = database
            .persist_station_session_if_revision(
                PersistStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some("session=stale".to_string()),
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: "web_authorization".to_string(),
                },
                old_revision,
                &data_key,
            )
            .expect_err("stale revision must be rejected");

        assert_eq!(error, "station_endpoint_revision_changed");
        let credentials = database
            .get_station_credentials(station.id)
            .expect("credentials");
        assert!(!credentials.cookie_present);
        assert!(!credentials.access_token_present);
    }

    #[test]
    fn newapi_session_invalidating_cookie_keeps_manual_access_token() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "newapi-scoped-invalidation");
        let data_key = [41_u8; 32];

        persist_dual_newapi_session(&database, &station.id, &data_key);
        database
            .invalidate_station_session_credential(
                &station.id,
                StationSessionCredentialKind::Cookie,
            )
            .expect("invalidate cookie");
        let credentials = database
            .get_station_credentials(station.id)
            .expect("credentials");

        assert!(credentials.access_token_present);
        assert!(!credentials.cookie_present);
        assert_eq!(credentials.session_source, "manual_token");
    }

    fn persist_dual_newapi_session(database: &AppDatabase, station_id: &str, data_key: &[u8; 32]) {
        database
            .persist_station_session_with_data_key(
                PersistStationSessionInput {
                    station_id: station_id.to_string(),
                    access_token: Some("manual-access-token".to_string()),
                    refresh_token: None,
                    cookie: Some("session=login-cookie".to_string()),
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: "manual_token".to_string(),
                },
                data_key,
            )
            .expect("persist dual session");
    }

    #[test]
    fn successful_collector_run_marks_station_as_collected() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-success-station-status");

        assert_eq!(station.status, "unchecked");

        let run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "balance".to_string(),
            })
            .expect("create run");

        database
            .finish_collector_run(FinishCollectorRunInput {
                id: run.id,
                status: "success".to_string(),
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                error_code: None,
                error_message: None,
                snapshot_id: None,
            })
            .expect("finish run");

        let updated = database
            .station_for_collector(&station.id)
            .expect("updated station");

        assert_eq!(updated.status, "healthy");
        assert!(updated.last_pricing_fetched_at.is_some());
    }

    #[test]
    fn repeated_collector_failure_preserves_active_event_read_state() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-repeat-failure");

        database
            .insert_collector_snapshot(
                &station.id,
                "sub2api-groups",
                "failed",
                json!({}),
                json!({}),
                None,
                Some("first failure".to_string()),
            )
            .expect("first failed snapshot");
        let first = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "collector_failed")
            .expect("collector failed event");
        let read = database
            .mark_change_event_read(first.id.clone())
            .expect("mark failure read");

        std::thread::sleep(std::time::Duration::from_millis(2));
        database
            .insert_collector_snapshot(
                &station.id,
                "sub2api-groups",
                "failed",
                json!({}),
                json!({}),
                None,
                Some("second failure".to_string()),
            )
            .expect("second failed snapshot");

        let repeated = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "collector_failed")
            .expect("repeated collector failed event");
        assert_eq!(repeated.id, first.id);
        assert_eq!(repeated.status, "read");
        assert_eq!(repeated.detected_at, first.detected_at);
        assert_eq!(repeated.updated_at, read.updated_at);
    }

    #[test]
    fn collector_failure_reactivates_after_recovery() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-failure-recovery");
        database
            .insert_collector_snapshot(
                &station.id,
                "sub2api-groups",
                "failed",
                json!({}),
                json!({}),
                None,
                Some("first failure".to_string()),
            )
            .expect("failed snapshot");
        let first = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "collector_failed")
            .expect("collector failed event");

        finish_test_collector_run(&database, &station.id, "groups", "failed");
        finish_test_collector_run(&database, &station.id, "groups", "success");
        let resolved = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "collector_failed")
            .expect("resolved collector failed event");
        assert_eq!(resolved.status, "resolved");
        assert!(resolved.resolved_at.is_some());

        std::thread::sleep(std::time::Duration::from_millis(2));
        database
            .insert_collector_snapshot(
                &station.id,
                "sub2api-groups",
                "failed",
                json!({}),
                json!({}),
                None,
                Some("second failure".to_string()),
            )
            .expect("failed snapshot after recovery");
        let reactivated = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "collector_failed")
            .expect("reactivated collector failed event");
        assert_eq!(reactivated.id, first.id);
        assert_eq!(reactivated.status, "unread");
        assert!(reactivated.resolved_at.is_none());
        assert_ne!(reactivated.detected_at, first.detected_at);
    }

    #[test]
    fn collector_failure_recovery_is_scoped_to_task_type() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-failure-task-scope");
        for task_type in ["groups", "balance"] {
            database
                .insert_collector_snapshot(
                    &station.id,
                    &format!("sub2api-{task_type}"),
                    "failed",
                    json!({}),
                    json!({}),
                    None,
                    Some(format!("{task_type} failure")),
                )
                .expect("failed snapshot");
        }

        finish_test_collector_run(&database, &station.id, "groups", "failed");
        finish_test_collector_run(&database, &station.id, "groups", "success");

        let events = database.list_change_events().expect("events");
        let groups_key = crate::services::change_events::collector_dedupe_key(
            &station.id,
            "collector_failed",
            "groups",
        );
        let balance_key = crate::services::change_events::collector_dedupe_key(
            &station.id,
            "collector_failed",
            "balance",
        );
        let groups_failure = events
            .iter()
            .find(|event| event.dedupe_key == groups_key)
            .expect("groups failure event");
        let balance_failure = events
            .iter()
            .find(|event| event.dedupe_key == balance_key)
            .expect("balance failure event");
        assert_eq!(groups_failure.status, "resolved");
        assert_eq!(balance_failure.status, "unread");
    }

    #[test]
    fn collector_recovery_is_scoped_to_the_same_task_type() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-cross-task-recovery");

        let groups_run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "groups".to_string(),
            })
            .expect("create failed groups run");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: groups_run.id,
                status: "failed".to_string(),
                endpoint_count: 1,
                success_count: 0,
                failure_count: 1,
                manual_action_required: false,
                error_code: Some("groups_failed".to_string()),
                error_message: Some("groups failed".to_string()),
                snapshot_id: None,
            })
            .expect("finish failed groups run");

        let balance_run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "balance".to_string(),
            })
            .expect("create successful balance run");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: balance_run.id,
                status: "success".to_string(),
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                error_code: None,
                error_message: None,
                snapshot_id: None,
            })
            .expect("finish successful balance run");

        let events = database.list_change_events().expect("events");
        assert!(
            events
                .iter()
                .all(|event| event.event_type != "collector_recovered"),
            "a successful balance task must not recover a previous groups failure"
        );
    }

    #[test]
    fn collector_recovery_event_records_the_recovered_task_type() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "collector-same-task-recovery");

        let failed_run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "balance".to_string(),
            })
            .expect("create failed balance run");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: failed_run.id,
                status: "failed".to_string(),
                endpoint_count: 1,
                success_count: 0,
                failure_count: 1,
                manual_action_required: false,
                error_code: Some("balance_failed".to_string()),
                error_message: Some("balance failed".to_string()),
                snapshot_id: None,
            })
            .expect("finish failed balance run");

        let recovered_run = database
            .create_collector_run(CreateCollectorRunInput {
                station_id: station.id.clone(),
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "balance".to_string(),
            })
            .expect("create recovered balance run");
        database
            .finish_collector_run(FinishCollectorRunInput {
                id: recovered_run.id,
                status: "success".to_string(),
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                error_code: None,
                error_message: None,
                snapshot_id: None,
            })
            .expect("finish recovered balance run");

        let events = database.list_change_events().expect("events");
        let recovered = events
            .iter()
            .find(|event| event.event_type == "collector_recovered")
            .expect("collector recovered event");
        assert!(recovered.dedupe_key.ends_with(":task:balance"));
        assert!(recovered
            .new_value_json
            .as_deref()
            .unwrap_or_default()
            .contains("\"taskType\":\"balance\""));
    }

    #[test]
    fn pricing_rule_round_trip() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "pricing-rule");

        let saved = database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: None,
                group_name: Some("pro".to_string()),
                tier_label: Some("tier-a".to_string()),
                model: "gpt-4o-mini".to_string(),
                input_price: Some(0.15),
                output_price: Some(0.6),
                fixed_price: None,
                rate_multiplier: None,
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: None,
                normalization_status: None,
                source: "manual".to_string(),
                confidence: 0.9,
                enabled: true,
                note: Some("manual override".to_string()),
                collected_at: Some("1000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("save");

        let rows = database.list_pricing_rules().expect("pricing rules");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, saved.id);
        assert_eq!(rows[0].station_id, station.id);
        assert_eq!(rows[0].model, "gpt-4o-mini");
        assert_eq!(rows[0].input_price, Some(0.15));

        database
            .delete_pricing_rule(saved.id)
            .expect("delete pricing rule");
        assert!(database
            .list_pricing_rules()
            .expect("pricing rules")
            .is_empty());
    }

    #[test]
    fn route_candidate_economics_prefers_requested_model_price_rule() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "pricing-rule-model-match");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "default-group".to_string(),
                input_price: None,
                output_price: None,
                fixed_price: None,
                rate_multiplier: Some(1.0),
                currency: "CNY".to_string(),
                unit: "multiplier".to_string(),
                price_type: "multiplier".to_string(),
                base_price_source: None,
                normalization_status: Some("group_rate_only".to_string()),
                source: "collector".to_string(),
                confidence: 0.8,
                enabled: true,
                note: None,
                collected_at: Some("1000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("group rate rule");
        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-5.4".to_string(),
                input_price: Some(2.0),
                output_price: Some(10.0),
                fixed_price: None,
                rate_multiplier: None,
                currency: "CNY".to_string(),
                unit: "per_1m_tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: Some("manual".to_string()),
                normalization_status: Some("complete".to_string()),
                source: "manual".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: Some("2000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("model price rule");

        let economics = database
            .route_candidate_economics_for_model(key.id, Some("gpt-5.4".to_string()))
            .expect("economics")
            .expect("model economics");

        assert_eq!(economics.pricing_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(economics.estimated_input_price, Some(2.0));
        assert_eq!(economics.estimated_output_price, Some(10.0));
        assert_eq!(economics.price_currency.as_deref(), Some("CNY"));
    }

    #[test]
    fn route_candidate_economics_uses_model_base_price_with_group_rate_only_rule() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "base-price-fallback");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("incentive".to_string()),
                tier_label: None,
                model: "gpt-5-mini".to_string(),
                input_price: None,
                output_price: None,
                fixed_price: None,
                rate_multiplier: Some(0.045),
                currency: "USD".to_string(),
                unit: "multiplier".to_string(),
                price_type: "multiplier".to_string(),
                base_price_source: None,
                normalization_status: Some("group_rate_only".to_string()),
                source: "collector".to_string(),
                confidence: 0.8,
                enabled: true,
                note: None,
                collected_at: Some("1000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("group rate rule");
        database
            .upsert_model_base_price(UpsertModelBasePriceInput {
                id: None,
                provider: "openai".to_string(),
                model: "gpt-5-mini".to_string(),
                input_price: Some(0.25),
                output_price: Some(2.0),
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                source_url: "https://developers.openai.com/api/docs/pricing".to_string(),
                source_label: "OpenAI API pricing".to_string(),
                source_checked_at: Some("2026-07-08".to_string()),
                enabled: true,
                built_in: false,
                note: None,
            })
            .expect("base price");

        let economics = database
            .route_candidate_economics_for_model(key.id, Some("gpt-5-mini".to_string()))
            .expect("economics")
            .expect("model economics");

        assert_eq!(economics.pricing_rule_id.as_deref(), None);
        assert_eq!(economics.pricing_model.as_deref(), Some("gpt-5-mini"));
        assert_eq!(economics.rate_multiplier, Some(0.045));
        assert_eq!(economics.estimated_input_price, Some(0.01125));
        assert_eq!(economics.estimated_output_price, Some(0.09));
        assert_eq!(
            economics.normalization_status.as_deref(),
            Some("base_price_with_group_rate")
        );
        assert_eq!(
            economics.pricing_source.as_deref(),
            Some("model_base_price")
        );
    }

    #[test]
    fn route_candidate_economics_uses_station_key_group_rate_without_pricing_rule() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "key-bound-group-rate");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let binding = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "group-hash-discount".to_string(),
                group_id_hash: Some("external-group-discount".to_string()),
                group_name: "discount".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.05),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.05),
                rate_source: Some("groups_api".to_string()),
                confidence: 0.9,
                last_seen_at: None,
                inferred_group_category: Some("unknown".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("binding");
        database
            .update_station_key_group_binding(UpdateStationKeyGroupBindingInput {
                station_key_id: key.id.clone(),
                group_binding_id: binding.id.clone(),
            })
            .expect("bind key");
        database
            .upsert_model_base_price(UpsertModelBasePriceInput {
                id: None,
                provider: "openai".to_string(),
                model: "key-bound-model".to_string(),
                input_price: Some(0.25),
                output_price: Some(2.0),
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                source_url: "https://developers.openai.com/api/docs/pricing".to_string(),
                source_label: "OpenAI API pricing".to_string(),
                source_checked_at: Some("2026-07-08".to_string()),
                enabled: true,
                built_in: false,
                note: None,
            })
            .expect("base price");

        let economics = database
            .route_candidate_economics_for_model(key.id, Some("key-bound-model".to_string()))
            .expect("economics")
            .expect("model economics");

        assert_eq!(economics.pricing_rule_id.as_deref(), None);
        assert_eq!(
            economics.group_binding_id.as_deref(),
            Some(binding.id.as_str())
        );
        assert_eq!(economics.rate_multiplier, Some(0.05));
        assert_eq!(economics.estimated_input_price, Some(0.0125));
        assert_eq!(economics.estimated_output_price, Some(0.1));
        assert_eq!(
            economics.normalization_status.as_deref(),
            Some("base_price_with_group_rate")
        );
    }

    #[test]
    fn route_candidate_economics_uses_model_base_price_without_pricing_rule() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "base-price-only");
        let key = database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0);

        let economics = database
            .route_candidate_economics_for_model(key.id, Some("gpt-5.4-mini".to_string()))
            .expect("economics")
            .expect("model base price economics");

        assert_eq!(economics.pricing_rule_id.as_deref(), None);
        assert_eq!(economics.pricing_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(economics.rate_multiplier, Some(1.0));
        assert_eq!(economics.estimated_input_price, Some(0.75));
        assert_eq!(economics.estimated_output_price, Some(4.5));
        assert_eq!(
            economics.normalization_status.as_deref(),
            Some("base_price_only")
        );
        assert_eq!(
            economics.pricing_source.as_deref(),
            Some("model_base_price")
        );
    }

    #[test]
    fn route_candidate_economics_ignores_manual_rule_for_other_model() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "manual-rule-other-model");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);

        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-other".to_string(),
                input_price: Some(99.0),
                output_price: Some(199.0),
                fixed_price: None,
                rate_multiplier: None,
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: Some("manual".to_string()),
                normalization_status: Some("complete".to_string()),
                source: "manual".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: Some("1000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("other model manual rule");

        let economics = database
            .route_candidate_economics_for_model(key.id, Some("gpt-5.4-mini".to_string()))
            .expect("economics")
            .expect("model base price economics");

        assert_eq!(economics.pricing_rule_id.as_deref(), None);
        assert_eq!(economics.pricing_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(economics.estimated_input_price, Some(0.75));
        assert_eq!(economics.estimated_output_price, Some(4.5));
        assert_eq!(
            economics.normalization_status.as_deref(),
            Some("base_price_only")
        );
    }

    #[test]
    fn balance_snapshot_round_trip() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "balance-snapshot");

        let saved = database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: None,
                station_id: station.id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(12.5),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: Some(7),
                total_request_count: Some(70),
                today_consumption: Some(0.25),
                total_consumption: Some(3.5),
                today_base_consumption: Some(0.5),
                total_base_consumption: Some(7.0),
                today_token_count: Some(1234),
                total_token_count: Some(56789),
                today_input_token_count: Some(1000),
                today_output_token_count: Some(234),
                total_input_token_count: Some(50000),
                total_output_token_count: Some(6789),
                account_concurrency_limit: None,
                low_balance_threshold: Some(5.0),
                status: "normal".to_string(),
                source: "collector".to_string(),
                confidence: 0.8,
                collected_at: Some("1000".to_string()),
            })
            .expect("save");

        let rows = database.list_balance_snapshots().expect("balances");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, saved.id);
        assert_eq!(rows[0].value, Some(12.5));
        assert_eq!(rows[0].today_request_count, Some(7));
        assert_eq!(rows[0].total_request_count, Some(70));
        assert_eq!(rows[0].today_consumption, Some(0.25));
        assert_eq!(rows[0].total_consumption, Some(3.5));
        assert_eq!(rows[0].today_base_consumption, Some(0.5));
        assert_eq!(rows[0].total_base_consumption, Some(7.0));
        assert_eq!(rows[0].today_token_count, Some(1234));
        assert_eq!(rows[0].total_token_count, Some(56789));
        assert_eq!(rows[0].today_input_token_count, Some(1000));
        assert_eq!(rows[0].today_output_token_count, Some(234));
        assert_eq!(rows[0].total_input_token_count, Some(50000));
        assert_eq!(rows[0].total_output_token_count, Some(6789));
        assert_eq!(rows[0].status, "normal");
    }
}
