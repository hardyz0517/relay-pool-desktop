use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::models::{
    change_events::{ChangeEvent, UpsertChangeEventInput},
    collector::CollectorSnapshot,
    credentials::{StationCredentials, UpdateStationCredentialsInput},
    pricing::{BalanceSnapshot, PricingRule, UpsertBalanceSnapshotInput, UpsertPricingRuleInput},
    proxy::{CreateRequestLogInput, RequestLog, UpstreamApiFormat},
    routing::{
        ModelAlias, RouteSimulationInput, RouteSimulationResult, RoutingPolicy,
        StationKeyCapabilities, StationKeyHealth, UpdateStationKeyCapabilitiesInput,
        UpsertModelAliasInput,
    },
    secrets::{SecretMigrationReport, SecretScanFinding},
    settings::{AppSettings, UpdateSettingsInput},
    station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
    stations::{CreateStationInput, Station, UpdateStationInput},
};
use crate::services::change_events::{
    STATUS_DISMISSED, STATUS_READ, STATUS_RESOLVED, STATUS_UNREAD,
};
use crate::services::proxy::{
    router::{select_route_candidates, RichRouteCandidate, RouteCandidateEconomics, RouteRequest},
    RouteCandidate,
};
use crate::services::secrets::{
    crypto::{decrypt_secret, encrypt_secret, EncryptedPayload},
    mask::{
        mask_secret as mask_sensitive_value, redact_text as redact_sensitive_text,
        redact_value as redact_sensitive_value,
    },
};

static NEXT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct AppDatabase {
    connection: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl AppDatabase {
    pub fn initialize(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法解析应用数据目录: {error}"))?;

        fs::create_dir_all(&data_dir)
            .map_err(|error| format!("无法创建应用数据目录 {}: {error}", data_dir.display()))?;

        let db_path = data_dir.join("relay-pool-desktop.sqlite3");
        let connection = Connection::open(&db_path)
            .map_err(|error| format!("无法打开 SQLite 数据库 {}: {error}", db_path.display()))?;

        initialize_schema(&connection)
            .map_err(|error| format!("初始化 SQLite schema 失败: {error}"))?;
        migrate_secret_schema(&connection)
            .map_err(|error| format!("迁移凭据安全字段失败: {error}"))?;
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_pricing_tables(&connection)
            .map_err(|error| format!("迁移价格和余额表失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;
        migrate_request_log_cost_columns(&connection)
            .map_err(|error| format!("迁移请求日志成本字段失败: {error}"))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            db_path,
        })
    }

    #[cfg(test)]
    pub fn new_in_memory_for_tests() -> Result<Self, String> {
        let connection = Connection::open_in_memory()
            .map_err(|error| format!("无法打开内存 SQLite 数据库: {error}"))?;
        initialize_schema(&connection)
            .map_err(|error| format!("初始化 SQLite schema 失败: {error}"))?;
        migrate_secret_schema(&connection)
            .map_err(|error| format!("迁移凭据安全字段失败: {error}"))?;
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_pricing_tables(&connection)
            .map_err(|error| format!("迁移价格和余额表失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;
        migrate_request_log_cost_columns(&connection)
            .map_err(|error| format!("迁移请求日志成本字段失败: {error}"))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            db_path: PathBuf::from(":memory:"),
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
            &input.base_url,
            input.credit_per_cny,
        )?;

        if input.api_key.trim().is_empty() {
            return Err("API Key 不能为空".to_string());
        }

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
            &input.base_url,
            input.credit_per_cny,
        )?;

        let connection = self.connection()?;
        update_station_in_connection(&connection, input, data_key)
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
        settings_from_connection(&connection, self.db_path.to_string_lossy().as_ref())
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

        let connection = self.connection()?;
        let values = [
            ("local_proxy_port", input.local_proxy_port.to_string()),
            ("default_routing_strategy", input.default_routing_strategy),
            (
                "low_balance_threshold_cny",
                input.low_balance_threshold_cny.to_string(),
            ),
            (
                "collector_interval_minutes",
                input.collector_interval_minutes.to_string(),
            ),
            ("tray_behavior", input.tray_behavior),
            (
                "developer_mode_enabled",
                input.developer_mode_enabled.to_string(),
            ),
        ];

        for (key, value) in values {
            upsert_setting(&connection, key, &value)?;
        }

        settings_from_connection(&connection, self.db_path.to_string_lossy().as_ref())
    }

    pub fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        let connection = self.connection()?;
        list_station_keys_from_connection(&connection, &station_id)
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
        let connection = self.connection()?;
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

        connection
            .execute("DELETE FROM station_keys WHERE id = ?1", params![id])
            .map_err(|error| format!("删除 Station Key 失败: {error}"))?;
        normalize_station_key_priorities(&connection, &station_id)?;
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

    pub fn route_candidate_economics(
        &self,
        station_key_id: String,
    ) -> Result<Option<RouteCandidateEconomics>, String> {
        let connection = self.connection()?;
        route_candidate_economics_by_station_key(&connection, &station_key_id)
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

    pub fn upsert_balance_snapshot(
        &self,
        input: UpsertBalanceSnapshotInput,
    ) -> Result<BalanceSnapshot, String> {
        let connection = self.connection()?;
        upsert_balance_snapshot_in_connection(&connection, input)
    }

    pub fn list_change_events(&self) -> Result<Vec<ChangeEvent>, String> {
        let connection = self.connection()?;
        list_change_events_from_connection(&connection)
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
        record_station_key_failure_in_connection(&connection, station_key_id, error_summary, now)
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

        CREATE INDEX IF NOT EXISTS idx_station_keys_station_priority
            ON station_keys(station_id, priority ASC, created_at ASC);

        CREATE TABLE IF NOT EXISTS collector_snapshots (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
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
            estimated_input_cost REAL,
            estimated_output_cost REAL,
            estimated_total_cost REAL,
            cost_currency TEXT,
            pricing_rule_id TEXT,
            pricing_source TEXT,
            cost_status TEXT,
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
    )
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
) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|existing| existing == column) {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}"),
            [],
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
    let defaults = [
        ("local_proxy_port", "8787"),
        ("local_key", "sk-local-pool-change-me"),
        ("default_routing_strategy", "manual"),
        ("low_balance_threshold_cny", "15"),
        ("collector_interval_minutes", "30"),
        ("tray_behavior", "minimize-to-tray"),
        ("developer_mode_enabled", "false"),
    ];

    for (key, value) in defaults {
        connection.execute(
            "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value, now_string()],
        )?;
    }

    Ok(())
}

fn row_to_station(row: &rusqlite::Row<'_>) -> rusqlite::Result<Station> {
    let api_key: String = row.get(4)?;
    let secret_masked: Option<String> = row.get(21)?;
    let api_key_secret_id: Option<String> = row.get(22)?;
    let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
    let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();

    Ok(Station {
        id: row.get(0)?,
        name: row.get(1)?,
        station_type: row.get(2)?,
        base_url: row.get(3)?,
        api_key_masked,
        api_key_present,
        key_count: row.get(7)?,
        enabled: i64_to_bool(row.get(8)?),
        priority: row.get(9)?,
        credit_per_cny: row.get(10)?,
        balance_raw: row.get(11)?,
        balance_cny: row.get(12)?,
        low_balance_threshold_cny: row.get(13)?,
        status: row.get(14)?,
        latency_ms: row.get(15)?,
        last_checked_at: row.get(16)?,
        last_pricing_fetched_at: row.get(17)?,
        note: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn list_stations_from_connection(connection: &Connection) -> Result<Vec<Station>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, station_type, base_url, api_key, upstream_api_format,
                    upstream_api_base_path,
                    (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
                    enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id),
                    api_key_secret_id
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
            "SELECT id, name, station_type, base_url, api_key, upstream_api_format,
                    upstream_api_base_path,
                    (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
                    enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at,
                    (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id),
                    api_key_secret_id
               FROM stations
              WHERE id = ?1",
            params![id],
            row_to_station,
        )
        .optional()
        .map_err(|error| format!("读取站点失败: {error}"))?
        .ok_or_else(|| "站点不存在".to_string())
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
    let stored_api_key = if data_key.is_some() {
        "".to_string()
    } else {
        plaintext_api_key.clone()
    };

    connection
        .execute(
            "INSERT INTO stations (
                id, name, station_type, base_url, api_key, api_key_secret_id, enabled, priority,
                credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                status, latency_ms, last_checked_at, last_pricing_fetched_at,
                note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, NULL, NULL, ?9,
                ?10, NULL, NULL, NULL, ?11, ?12, ?13)",
            params![
                id,
                input.name.trim(),
                input.station_type,
                input.base_url.trim(),
                stored_api_key,
                bool_to_i64(input.enabled),
                next_priority,
                input.credit_per_cny,
                input.low_balance_threshold_cny,
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

    if let Some(data_key) = data_key {
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

    create_station_key_in_connection_with_data_key(
        connection,
        CreateStationKeyInput {
            station_id: id.clone(),
            name: "Default Key".to_string(),
            api_key: input.api_key,
            enabled: input.enabled,
            priority: Some(0),
            group_name: None,
            tier_label: None,
            note: Some("由站点默认 API Key 创建。".to_string()),
        },
        data_key,
    )?;

    station_by_id(connection, &id)
}

fn update_station_in_connection(
    connection: &Connection,
    input: UpdateStationInput,
    data_key: Option<&[u8; 32]>,
) -> Result<Station, String> {
    let existing: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT api_key, api_key_secret_id FROM stations WHERE id = ?1",
            params![input.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取站点 API Key 失败: {error}"))?;

    let Some((existing_api_key, existing_secret_id)) = existing else {
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
    let now = now_string();

    connection
        .execute(
            "UPDATE stations
                SET name = ?1,
                    station_type = ?2,
                    base_url = ?3,
                    api_key = ?4,
                    api_key_secret_id = ?5,
                    enabled = ?6,
                    credit_per_cny = ?7,
                    low_balance_threshold_cny = ?8,
                    status = CASE WHEN ?6 = 0 THEN 'disabled'
                                  WHEN status = 'disabled' THEN 'unchecked'
                                  ELSE status END,
                    note = ?9,
                    updated_at = ?10
              WHERE id = ?11",
            params![
                input.name.trim(),
                input.station_type,
                input.base_url.trim(),
                next_api_key,
                next_secret_id,
                bool_to_i64(input.enabled),
                input.credit_per_cny,
                input.low_balance_threshold_cny,
                normalize_optional_string(input.note),
                now,
                input.id,
            ],
        )
        .map_err(|error| format!("更新站点失败: {error}"))?;

    station_by_id(connection, &input.id)
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
    if !rows.iter().any(|column| column == "upstream_api_base_path") {
        connection.execute(
            "ALTER TABLE stations ADD COLUMN upstream_api_base_path TEXT NOT NULL DEFAULT '/v1'",
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
    let secret_masked: Option<String> = row.get(14)?;
    let api_key_secret_id: Option<String> = row.get(15)?;
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
        group_name: row.get(6)?,
        tier_label: row.get(7)?,
        status: row.get(8)?,
        last_checked_at: row.get(9)?,
        last_used_at: row.get(10)?,
        note: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn list_station_keys_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<StationKey>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, name, api_key, enabled, priority, group_name, tier_label,
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
                s.base_url,
                k.name,
                k.api_key,
                (SELECT masked_value FROM secrets WHERE secrets.id = k.api_key_secret_id),
                k.api_key_secret_id,
                k.enabled,
                k.priority,
                k.group_name,
                k.tier_label,
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
                h.last_error_summary
             FROM station_keys k
             INNER JOIN stations s ON s.id = k.station_id
             LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
             LEFT JOIN station_key_health h ON h.station_key_id = k.id
             ORDER BY k.priority ASC, k.created_at ASC",
        )
        .map_err(|error| format!("读取 Key 池失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let api_key: String = row.get(6)?;
            let secret_masked: Option<String> = row.get(7)?;
            let api_key_secret_id: Option<String> = row.get(8)?;
            let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
            let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();
            let supports_chat = i64_to_bool(row.get(19)?);
            let supports_responses = i64_to_bool(row.get(20)?);
            let supports_embeddings = i64_to_bool(row.get(21)?);
            let supports_stream = i64_to_bool(row.get(22)?);
            let supports_tools = i64_to_bool(row.get(23)?);
            let supports_vision = i64_to_bool(row.get(24)?);
            let supports_reasoning = i64_to_bool(row.get(25)?);
            let allowlist = parse_json_string_list(row.get::<_, String>(26)?.as_str());
            let blocklist = parse_json_string_list(row.get::<_, String>(27)?.as_str());
            let preferred_models = parse_json_string_list(row.get::<_, String>(28)?.as_str());
            let success_count = row.get::<_, Option<i64>>(31)?.unwrap_or(0);
            let failure_count = row.get::<_, Option<i64>>(32)?.unwrap_or(0);
            Ok(KeyPoolItem {
                id: row.get(0)?,
                station_id: row.get(1)?,
                station_name: row.get(2)?,
                station_type: row.get(3)?,
                station_base_url: row.get(4)?,
                name: row.get(5)?,
                api_key_masked,
                api_key_present,
                enabled: i64_to_bool(row.get(9)?),
                priority: row.get(10)?,
                group_name: row.get(11)?,
                tier_label: row.get(12)?,
                status: row.get(13)?,
                last_checked_at: row.get(14)?,
                last_used_at: row.get(15)?,
                note: row.get(16)?,
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
                only_use_as_backup: i64_to_bool(row.get(29)?),
                cooldown_until: row.get(30)?,
                success_rate: success_rate(success_count, failure_count),
                avg_latency_ms: row.get(33)?,
                consecutive_failures: row.get(34)?,
                last_error_summary: row.get(35)?,
                created_at: row.get(17)?,
                updated_at: row.get(18)?,
            })
        })
        .map_err(|error| format!("查询 Key 池失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Key 池失败: {error}"))?;

    Ok(rows)
}

fn station_key_capabilities_by_id(
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

fn update_station_key_capabilities_in_connection(
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
            "SELECT station_key_id, last_success_at, last_failure_at, consecutive_failures,
                    success_count, failure_count, avg_latency_ms, last_error_summary,
                    cooldown_until, updated_at
               FROM station_key_health
              ORDER BY updated_at DESC",
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
            "SELECT station_key_id, last_success_at, last_failure_at, consecutive_failures,
                    success_count, failure_count, avg_latency_ms, last_error_summary,
                    cooldown_until, updated_at
               FROM station_key_health
              WHERE station_key_id = ?1",
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
                station_key_id, last_success_at, last_failure_at, consecutive_failures,
                success_count, failure_count, total_duration_ms, avg_latency_ms,
                last_error_summary, cooldown_until, updated_at
             ) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, NULL, NULL, ?8)
             ON CONFLICT(station_key_id) DO UPDATE SET
                last_success_at = excluded.last_success_at,
                consecutive_failures = 0,
                success_count = excluded.success_count,
                total_duration_ms = excluded.total_duration_ms,
                avg_latency_ms = excluded.avg_latency_ms,
                last_error_summary = NULL,
                cooldown_until = NULL,
                updated_at = excluded.updated_at",
            params![
                station_key_id,
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
) -> Result<(), String> {
    validate_station_key_exists(connection, station_key_id)?;
    let current = station_key_health_by_id(connection, station_key_id)?;
    let consecutive_failures = current.consecutive_failures + 1;
    let failure_count = current.failure_count + 1;
    let cooldown_until = cooldown_until(consecutive_failures, now);

    connection
        .execute(
            "INSERT INTO station_key_health (
                station_key_id, last_success_at, last_failure_at, consecutive_failures,
                success_count, failure_count, total_duration_ms, avg_latency_ms,
                last_error_summary, cooldown_until, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(station_key_id) DO UPDATE SET
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
        if let Some(event) = crate::services::change_events::key_health_event(
            station_key_id,
            &station_id,
            consecutive_failures,
            Some(&trim_error_summary(error_summary)),
            cooldown_until.as_deref(),
        ) {
            let _ = upsert_change_event_in_connection(connection, event);
        }
    }

    Ok(())
}

fn cooldown_until(consecutive_failures: i64, now: &str) -> Option<String> {
    let now = now.parse::<i64>().ok()?;
    let duration_ms = match consecutive_failures {
        failures if failures < 3 => return None,
        3 => 2 * 60 * 1000,
        4 => 5 * 60 * 1000,
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
    let mut statement = connection
        .prepare(
            "SELECT k.id, k.station_id, s.base_url, k.api_key, k.api_key_secret_id,
                    s.upstream_api_format, k.priority
               FROM station_keys k
               JOIN stations s ON s.id = k.station_id
              WHERE k.enabled = 1
                AND s.enabled = 1
                AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
              ORDER BY k.priority ASC, k.created_at ASC",
        )
        .map_err(|error| format!("读取 Key 池候选失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let station_key_id: String = row.get(0)?;
            let api_key: String = row.get(3)?;
            Ok(RouteCandidate {
                station_key_id,
                station_id: row.get(1)?,
                upstream_base_url: row.get(2)?,
                api_key,
                upstream_api_format: parse_upstream_api_format(row.get::<_, String>(5)?),
                priority: row.get(6)?,
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
    let mut statement = connection
        .prepare(
            "SELECT
                k.id,
                k.station_id,
                s.base_url,
                k.api_key,
                k.api_key_secret_id,
                s.upstream_api_format,
                k.priority,
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
             ORDER BY k.priority ASC, k.created_at ASC",
        )
        .map_err(|error| format!("读取富路由候选失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let station_key_id = row.get::<_, String>(0)?;
            let api_key: String = row.get(3)?;
            let health_station_key_id = row.get::<_, Option<String>>(22)?;
            Ok(RichRouteCandidate {
                candidate: RouteCandidate {
                    station_key_id: station_key_id.clone(),
                    station_id: row.get(1)?,
                    upstream_base_url: row.get(2)?,
                    api_key,
                    upstream_api_format: parse_upstream_api_format(row.get::<_, String>(5)?),
                    priority: row.get(6)?,
                },
                station_name: row.get(7)?,
                key_name: row.get(8)?,
                capabilities: StationKeyCapabilities {
                    station_key_id,
                    supports_chat_completions: i64_to_bool(row.get(9)?),
                    supports_responses: i64_to_bool(row.get(10)?),
                    supports_embeddings: i64_to_bool(row.get(11)?),
                    supports_stream: i64_to_bool(row.get(12)?),
                    supports_tools: i64_to_bool(row.get(13)?),
                    supports_vision: i64_to_bool(row.get(14)?),
                    supports_reasoning: i64_to_bool(row.get(15)?),
                    model_allowlist: parse_json_string_list(row.get::<_, String>(16)?.as_str()),
                    model_blocklist: parse_json_string_list(row.get::<_, String>(17)?.as_str()),
                    preferred_models: parse_json_string_list(row.get::<_, String>(18)?.as_str()),
                    only_use_as_backup: i64_to_bool(row.get(19)?),
                    routing_tags: parse_json_string_list(row.get::<_, String>(20)?.as_str()),
                    updated_at: row.get(21)?,
                },
                health: health_station_key_id.map(|station_key_id| StationKeyHealth {
                    station_key_id,
                    last_success_at: row.get(23).ok().flatten(),
                    last_failure_at: row.get(24).ok().flatten(),
                    consecutive_failures: row.get(25).unwrap_or(0),
                    success_count: row.get(26).unwrap_or(0),
                    failure_count: row.get(27).unwrap_or(0),
                    avg_latency_ms: row.get(28).ok().flatten(),
                    last_error_summary: row.get(29).ok().flatten(),
                    cooldown_until: row.get(30).ok().flatten(),
                    updated_at: row.get(31).unwrap_or_else(|_| "0".to_string()),
                }),
                economics: None,
            })
        })
        .map_err(|error| format!("查询富路由候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析富路由候选失败: {error}"))?;

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
        )?;
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
) -> Result<Option<RouteCandidateEconomics>, String> {
    let pricing_rule = connection
        .query_row(
            "SELECT id, model, input_price, output_price, fixed_price, currency, source
               FROM pricing_rules
              WHERE station_id = ?1
                AND enabled = 1
              ORDER BY updated_at DESC, created_at DESC
              LIMIT 1",
            params![station_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取价格规则失败: {error}"))?;

    let balance_snapshot = connection
        .query_row(
            "SELECT value, currency, low_balance_threshold, status
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
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取余额快照失败: {error}"))?;

    let economics = pricing_rule
        .map(
            |(id, model, input_price, output_price, fixed_price, currency, source)| {
                let (balance_value, balance_currency, low_balance_threshold, balance_status) =
                    balance_snapshot.clone().unwrap_or((
                        None,
                        "unknown".to_string(),
                        None,
                        "unknown".to_string(),
                    ));
                RouteCandidateEconomics {
                    pricing_rule_id: Some(id),
                    pricing_model: Some(model),
                    estimated_input_price: input_price,
                    estimated_output_price: output_price,
                    fixed_price,
                    price_currency: Some(currency),
                    pricing_source: Some(source),
                    balance_status: Some(balance_status),
                    balance_value,
                    low_balance_threshold,
                    balance_currency: Some(balance_currency),
                }
            },
        )
        .or_else(|| {
            balance_snapshot.map(
                |(balance_value, balance_currency, low_balance_threshold, balance_status)| {
                    RouteCandidateEconomics {
                        pricing_rule_id: None,
                        pricing_model: None,
                        estimated_input_price: None,
                        estimated_output_price: None,
                        fixed_price: None,
                        price_currency: None,
                        pricing_source: None,
                        balance_status: Some(balance_status),
                        balance_value,
                        low_balance_threshold,
                        balance_currency: Some(balance_currency),
                    }
                },
            )
        });

    Ok(economics)
}

fn route_candidate_economics_by_station_key(
    connection: &Connection,
    station_key_id: &str,
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
    route_candidate_economics_from_connection(connection, station_key_id, &station_id)
}

fn list_pricing_rules_from_connection(connection: &Connection) -> Result<Vec<PricingRule>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, group_name, tier_label, model, input_price, output_price,
                    fixed_price, currency, unit, price_type, source, confidence, enabled, note,
                    collected_at, created_at, updated_at
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
    validate_station_exists(connection, &input.station_id)?;
    if input.model.trim().is_empty() {
        return Err("模型不能为空".to_string());
    }
    let previous_rule = connection
        .query_row(
            "SELECT id, station_id, group_name, tier_label, model, input_price, output_price,
                    fixed_price, currency, unit, price_type, source, confidence, enabled, note,
                    collected_at, created_at, updated_at
               FROM pricing_rules
              WHERE station_id = ?1 AND COALESCE(group_name, '') = COALESCE(?2, '') AND model = ?3
              ORDER BY updated_at DESC
              LIMIT 1",
            params![
                &input.station_id,
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
                id, station_id, group_name, tier_label, model, input_price, output_price,
                fixed_price, currency, unit, price_type, source, confidence, enabled, note,
                collected_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                group_name = excluded.group_name,
                tier_label = excluded.tier_label,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                fixed_price = excluded.fixed_price,
                currency = excluded.currency,
                unit = excluded.unit,
                price_type = excluded.price_type,
                source = excluded.source,
                confidence = excluded.confidence,
                enabled = excluded.enabled,
                note = excluded.note,
                collected_at = excluded.collected_at,
                updated_at = excluded.updated_at",
            params![
                id,
                input.station_id,
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
                input.model.trim(),
                input.input_price,
                input.output_price,
                input.fixed_price,
                normalize_currency(input.currency),
                normalize_unit(input.unit),
                input.price_type.trim(),
                input.source.trim(),
                confidence,
                bool_to_i64(input.enabled),
                normalize_optional_string(input.note),
                normalize_optional_string(input.collected_at),
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存价格规则失败: {error}"))?;
    let saved = pricing_rule_by_id(connection, &id)?;
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
                    used_value, total_value, low_balance_threshold, status, source, confidence,
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

fn upsert_balance_snapshot_in_connection(
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
                used_value, total_value, low_balance_threshold, status, source, confidence,
                collected_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                scope = excluded.scope,
                value = excluded.value,
                currency = excluded.currency,
                credit_unit = excluded.credit_unit,
                used_value = excluded.used_value,
                total_value = excluded.total_value,
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
        group_name: row.get(2)?,
        tier_label: row.get(3)?,
        model: row.get(4)?,
        input_price: row.get(5)?,
        output_price: row.get(6)?,
        fixed_price: row.get(7)?,
        currency: row.get(8)?,
        unit: row.get(9)?,
        price_type: row.get(10)?,
        source: row.get(11)?,
        confidence: row.get(12)?,
        enabled: i64_to_bool(row.get(13)?),
        note: row.get(14)?,
        collected_at: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
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
        low_balance_threshold: row.get(9)?,
        status: row.get(10)?,
        source: row.get(11)?,
        confidence: row.get(12)?,
        collected_at: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn pricing_rule_by_id(connection: &Connection, id: &str) -> Result<PricingRule, String> {
    connection
        .query_row(
            "SELECT id, station_id, group_name, tier_label, model, input_price, output_price,
                    fixed_price, currency, unit, price_type, source, confidence, enabled, note,
                    collected_at, created_at, updated_at
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
                    used_value, total_value, low_balance_threshold, status, source, confidence,
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
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
               FROM change_events
              ORDER BY updated_at DESC, detected_at DESC",
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_change_event)
        .map_err(|error| format!("查询变更事件失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析变更事件失败: {error}"))?;
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
                detected_at = excluded.detected_at,
                resolved_at = NULL,
                updated_at = excluded.updated_at",
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
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
               FROM change_events
              WHERE dedupe_key = ?1",
            params![dedupe_key],
            row_to_change_event,
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))
}

fn change_event_by_id(connection: &Connection, id: &str) -> Result<ChangeEvent, String> {
    connection
        .query_row(
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
               FROM change_events
              WHERE id = ?1",
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
        station_key_id: row.get(9)?,
        pricing_rule_id: row.get(10)?,
        request_log_id: row.get(11)?,
        old_value_json: row.get(12)?,
        new_value_json: row.get(13)?,
        impact_json: row.get(14)?,
        dedupe_key: row.get(15)?,
        source: row.get(16)?,
        detected_at: row.get(17)?,
        resolved_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
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
    let policy = input.policy.unwrap_or_else(|| {
        settings_from_connection(connection, data_dir)
            .map(|settings| parse_routing_policy_value(&settings.default_routing_strategy))
            .unwrap_or(RoutingPolicy::PriorityFallback)
    });
    let request = RouteRequest {
        endpoint: input.endpoint,
        model: input.model,
        stream: input.stream,
        uses_tools: input.uses_tools,
        uses_vision: input.uses_vision,
        uses_reasoning: input.uses_reasoning,
        policy: policy.clone(),
        now_ms: now_millis_for_services() as i64,
    };
    let candidates = proxy_rich_route_candidates_from_connection(connection)?;
    let aliases = enabled_model_alias_pairs_from_connection(connection)?;
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
        candidates: selection.explanations,
        message,
    })
}

fn parse_routing_policy_value(value: &str) -> RoutingPolicy {
    match value {
        "stable_first" | "stable" => RoutingPolicy::StableFirst,
        "backup_only" => RoutingPolicy::BackupOnly,
        "cheap_first" => RoutingPolicy::CheapFirst,
        _ => RoutingPolicy::PriorityFallback,
    }
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
    connection
        .execute(
            "INSERT INTO request_logs (
                id, started_at, finished_at, duration_ms, method, path, model, stream,
                status, station_key_id, station_id, upstream_base_url, fallback_count,
                error_message, route_policy, route_reason, rejected_candidates_json,
                prompt_tokens, completion_tokens, total_tokens, estimated_input_cost,
                estimated_output_cost, estimated_total_cost, cost_currency, pricing_rule_id,
                pricing_source, cost_status, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
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
                input.estimated_input_cost,
                input.estimated_output_cost,
                input.estimated_total_cost,
                normalize_optional_string(input.cost_currency),
                normalize_optional_string(input.pricing_rule_id),
                normalize_optional_string(input.pricing_source),
                normalize_optional_string(input.cost_status),
                created_at,
            ],
        )
        .map_err(|error| format!("保存请求日志失败: {error}"))?;

    request_log_by_id(connection, &id)
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
        station_key_id: row.get(9)?,
        station_id: row.get(10)?,
        upstream_base_url: row.get(11)?,
        fallback_count: row.get(12)?,
        error_message: row.get(13)?,
        route_policy: row.get(14)?,
        route_reason: row.get(15)?,
        rejected_candidates_json: row.get(16)?,
        prompt_tokens: row.get(17)?,
        completion_tokens: row.get(18)?,
        total_tokens: row.get(19)?,
        estimated_input_cost: row.get(20)?,
        estimated_output_cost: row.get(21)?,
        estimated_total_cost: row.get(22)?,
        cost_currency: row.get(23)?,
        pricing_rule_id: row.get(24)?,
        pricing_source: row.get(25)?,
        cost_status: row.get(26)?,
        created_at: row.get(27)?,
    })
}

fn request_log_by_id(connection: &Connection, id: &str) -> Result<RequestLog, String> {
    connection
        .query_row(
            "SELECT id, started_at, finished_at, duration_ms, method, path, model, stream,
                    status, station_key_id, station_id, upstream_base_url, fallback_count,
                    error_message, route_policy, route_reason, rejected_candidates_json,
                    prompt_tokens, completion_tokens, total_tokens, estimated_input_cost,
                    estimated_output_cost, estimated_total_cost, cost_currency, pricing_rule_id,
                    pricing_source, cost_status, created_at
               FROM request_logs
              WHERE id = ?1",
            params![id],
            row_to_request_log,
        )
        .optional()
        .map_err(|error| format!("读取请求日志失败: {error}"))?
        .ok_or_else(|| "请求日志不存在".to_string())
}

fn list_request_logs_from_connection(connection: &Connection) -> Result<Vec<RequestLog>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, started_at, finished_at, duration_ms, method, path, model, stream,
                    status, station_key_id, station_id, upstream_base_url, fallback_count,
                    error_message, route_policy, route_reason, rejected_candidates_json,
                    prompt_tokens, completion_tokens, total_tokens, estimated_input_cost,
                    estimated_output_cost, estimated_total_cost, cost_currency, pricing_rule_id,
                    pricing_source, cost_status, created_at
               FROM request_logs
              ORDER BY created_at DESC
              LIMIT 500",
        )
        .map_err(|error| format!("读取请求日志列表失败: {error}"))?;

    let rows = statement
        .query_map([], row_to_request_log)
        .map_err(|error| format!("查询请求日志失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析请求日志失败: {error}"))?;
    Ok(rows)
}

fn create_station_key_in_connection(
    connection: &Connection,
    input: CreateStationKeyInput,
) -> Result<StationKey, String> {
    create_station_key_in_connection_with_data_key(connection, input, None)
}

fn create_station_key_in_connection_with_data_key(
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

    connection
        .execute(
            "INSERT INTO station_keys (
                id, station_id, name, api_key, api_key_secret_id, enabled, priority, group_name, tier_label,
                status, last_checked_at, last_used_at, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'unchecked', NULL, NULL, ?10, ?11, ?12)",
            params![
                id,
                input.station_id,
                input.name.trim(),
                stored_api_key,
                secret_id,
                bool_to_i64(input.enabled),
                priority,
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
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

fn update_station_key_in_connection_with_data_key(
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
    let now = now_string();

    connection
        .execute(
            "UPDATE station_keys
                SET name = ?1,
                    api_key = ?2,
                    api_key_secret_id = ?3,
                    enabled = ?4,
                    priority = ?5,
                    group_name = ?6,
                    tier_label = ?7,
                    status = ?8,
                    note = ?9,
                    updated_at = ?10
              WHERE id = ?11 AND station_id = ?12",
            params![
                input.name.trim(),
                next_api_key,
                next_secret_id,
                bool_to_i64(input.enabled),
                input.priority,
                normalize_optional_string(input.group_name),
                normalize_optional_string(input.tier_label),
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

fn station_key_by_id(connection: &Connection, id: &str) -> Result<StationKey, String> {
    connection
        .query_row(
            "SELECT id, station_id, name, api_key, enabled, priority, group_name, tier_label,
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
            "SELECT station_id, login_username, login_password, login_password_secret_id, remember_password,
                    login_status, login_error, last_login_at, session_status,
                    session_expires_at, updated_at
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                let password: Option<String> = row.get(2)?;
                let password_secret_id: Option<String> = row.get(3)?;
                Ok(StationCredentials {
                    station_id: row.get(0)?,
                    login_username: row.get(1)?,
                    password_present: password_secret_id.is_some()
                        || password
                        .map(|value| !value.trim().is_empty())
                        .unwrap_or(false),
                    remember_password: i64_to_bool(row.get(4)?),
                    login_status: row.get(5)?,
                    login_error: row.get(6)?,
                    last_login_at: row.get(7)?,
                    session_status: row.get(8)?,
                    session_expires_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取登录信息失败: {error}"))?;

    Ok(credentials.unwrap_or_else(|| StationCredentials {
        station_id: station_id.to_string(),
        login_username: None,
        password_present: false,
        remember_password: false,
        login_status: "unknown".to_string(),
        login_error: None,
        last_login_at: None,
        session_status: "none".to_string(),
        session_expires_at: None,
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
    let id = generate_id("snapshot");
    let now = now_string();
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
                id, station_id, source, status, fetched_at, summary_json,
                normalized_json, raw_json_redacted, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                station_id,
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
    if saved.status == "failed" {
        let event = crate::services::change_events::collector_failed_event(
            &saved.station_id,
            saved.error_message.as_deref(),
        );
        let _ = upsert_change_event_in_connection(connection, event);
    }
    if let Some(previous_snapshot) = previous_snapshot.as_ref() {
        let previous_models = models_from_snapshot_value(&previous_snapshot.normalized_json);
        let next_models = models_from_snapshot_value(&saved.normalized_json);
        for model in next_models.iter().filter(|model| !previous_models.contains(model)) {
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
        for model in previous_models.iter().filter(|model| !next_models.contains(model)) {
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
                impact_json: Some(json!({ "routingRisk": "model_candidates_may_change" }).to_string()),
                dedupe_key: crate::services::change_events::model_dedupe_key(
                    &saved.station_id,
                    "model_removed",
                    model,
                ),
                source: "collector".to_string(),
            };
            let _ = upsert_change_event_in_connection(connection, event);
        }

        let previous_rates = rate_multipliers_from_snapshot_value(&previous_snapshot.normalized_json);
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
    Ok(saved)
}

fn row_to_collector_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectorSnapshot> {
    let summary_string: String = row.get(5)?;
    let normalized_string: String = row.get(6)?;
    let raw_string: Option<String> = row.get(7)?;

    Ok(CollectorSnapshot {
        id: row.get(0)?,
        station_id: row.get(1)?,
        source: row.get(2)?,
        status: row.get(3)?,
        fetched_at: row.get(4)?,
        summary_json: parse_json_value(&summary_string),
        normalized_json: parse_json_value(&normalized_string),
        raw_json_redacted: raw_string.as_deref().map(parse_json_value),
        error_message: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn collector_snapshot_by_id(
    connection: &Connection,
    id: &str,
) -> Result<CollectorSnapshot, String> {
    connection
        .query_row(
            "SELECT id, station_id, source, status, fetched_at, summary_json,
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
            "SELECT id, station_id, source, status, fetched_at, summary_json,
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
            "SELECT id, station_id, source, status, fetched_at, summary_json,
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
                    model
                        .as_str()
                        .map(ToString::to_string)
                        .or_else(|| model.get("id").and_then(Value::as_str).map(ToString::to_string))
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

fn settings_from_connection(
    connection: &Connection,
    data_dir: &str,
) -> Result<AppSettings, String> {
    let local_key = read_setting(connection, "local_key")?;

    Ok(AppSettings {
        local_proxy_port: parse_setting(connection, "local_proxy_port")?,
        local_key_masked: mask_secret(&local_key),
        default_routing_strategy: read_setting(connection, "default_routing_strategy")?,
        low_balance_threshold_cny: parse_setting(connection, "low_balance_threshold_cny")?,
        collector_interval_minutes: parse_setting(connection, "collector_interval_minutes")?,
        tray_behavior: read_setting(connection, "tray_behavior")?,
        developer_mode_enabled: read_setting_or_default(
            connection,
            "developer_mode_enabled",
            "false",
        )?
        .parse()
        .map_err(|_| "设置项 developer_mode_enabled 格式无效".to_string())?,
        data_dir: data_dir.to_string(),
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
    base_url: &str,
    credit_per_cny: f64,
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
    if base_url.trim().is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    if credit_per_cny <= 0.0 {
        return Err("充值兑换比例必须大于 0".to_string());
    }

    Ok(())
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
    use crate::models::pricing::{UpsertBalanceSnapshotInput, UpsertPricingRuleInput};
    use crate::models::routing::RouteEndpointKind;

    fn test_station(database: &AppDatabase, name: &str) -> Station {
        database
            .create_station(CreateStationInput {
                name: name.to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: "https://example.test".to_string(),
                api_key: "sk-test-routing".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                note: None,
            })
            .expect("station")
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
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-test".to_string(),
                input_price: Some(1.0),
                output_price: Some(2.0),
                fixed_price: None,
                currency: "USD".to_string(),
                unit: "1M tokens".to_string(),
                price_type: "token".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: None,
            })
            .expect("old price");
        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-test".to_string(),
                input_price: Some(1.0),
                output_price: Some(3.0),
                fixed_price: None,
                currency: "USD".to_string(),
                unit: "1M tokens".to_string(),
                price_type: "token".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: None,
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
    fn request_log_records_route_policy_and_reason_without_prompt() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let log = database
            .insert_request_log(CreateRequestLogInput {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some("gpt-5.4".to_string()),
                stream: false,
                status: "success".to_string(),
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
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
            })
            .expect("insert log");

        assert_eq!(log.route_policy.as_deref(), Some("priority_fallback"));
        assert_eq!(
            log.route_reason.as_deref(),
            Some("selected key-1 because model allowed")
        );
        assert_eq!(log.rejected_candidates_json.as_deref(), Some("[]"));
        let serialized = serde_json::to_string(&log).unwrap();
        assert!(!serialized.contains("\"prompt\":"));
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
                estimated_input_cost: None,
                estimated_output_cost: None,
                estimated_total_cost: None,
                cost_currency: None,
                pricing_rule_id: None,
                pricing_source: None,
                cost_status: None,
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
                group_name: None,
                tier_label: None,
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
                group_name: None,
                tier_label: None,
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
                group_name: None,
                tier_label: None,
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
                    group_name: None,
                    tier_label: None,
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
                    group_name: None,
                    tier_label: None,
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
                    group_name: None,
                    tier_label: None,
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
    fn pricing_rule_round_trip() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "pricing-rule");

        let saved = database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: station.id.clone(),
                group_name: Some("pro".to_string()),
                tier_label: Some("tier-a".to_string()),
                model: "gpt-4o-mini".to_string(),
                input_price: Some(0.15),
                output_price: Some(0.6),
                fixed_price: None,
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                price_type: "token".to_string(),
                source: "manual".to_string(),
                confidence: 0.9,
                enabled: true,
                note: Some("manual override".to_string()),
                collected_at: Some("1000".to_string()),
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
        assert_eq!(rows[0].status, "normal");
    }
}
