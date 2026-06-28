use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::models::{
    collector::CollectorSnapshot,
    credentials::{StationCredentials, UpdateStationCredentialsInput},
    proxy::{CreateRequestLogInput, RequestLog, UpstreamApiFormat},
    routing::{
        ModelAlias, StationKeyCapabilities, StationKeyHealth, UpdateStationKeyCapabilitiesInput,
        UpsertModelAliasInput,
    },
    settings::{AppSettings, UpdateSettingsInput},
    station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
    stations::{CreateStationInput, Station, UpdateStationInput},
};
use crate::services::proxy::{router::RichRouteCandidate, RouteCandidate};

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
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            db_path,
        })
    }

    #[cfg(test)]
    pub fn new_in_memory_for_tests() -> Result<Self, String> {
        let connection =
            Connection::open_in_memory().map_err(|error| format!("无法打开内存 SQLite 数据库: {error}"))?;
        initialize_schema(&connection)
            .map_err(|error| format!("初始化 SQLite schema 失败: {error}"))?;
        seed_default_settings(&connection)
            .map_err(|error| format!("初始化默认设置失败: {error}"))?;
        migrate_default_station_keys(&connection)
            .map_err(|error| format!("迁移默认站点 Key 失败: {error}"))?;
        migrate_station_proxy_columns(&connection)
            .map_err(|error| format!("迁移站点代理字段失败: {error}"))?;
        migrate_request_log_route_columns(&connection)
            .map_err(|error| format!("迁移请求日志路由字段失败: {error}"))?;

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

    pub fn list_stations(&self) -> Result<Vec<Station>, String> {
        let connection = self.connection()?;
        list_stations_from_connection(&connection)
    }

    pub fn create_station(&self, input: CreateStationInput) -> Result<Station, String> {
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
        let id = generate_id("station");
        let now = now_string();
        let next_priority = next_station_priority(&connection)?;

        connection
            .execute(
                "INSERT INTO stations (
                    id, name, station_type, base_url, api_key, enabled, priority,
                    credit_per_cny, balance_raw, balance_cny, low_balance_threshold_cny,
                    status, latency_ms, last_checked_at, last_pricing_fetched_at,
                    note, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, ?9,
                    ?10, NULL, NULL, NULL, ?11, ?12, ?13)",
                params![
                    id,
                    input.name.trim(),
                    input.station_type,
                    input.base_url.trim(),
                    input.api_key.trim(),
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

        create_station_key_in_connection(
            &connection,
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
        )?;

        station_by_id(&connection, &id)
    }

    pub fn update_station(&self, input: UpdateStationInput) -> Result<Station, String> {
        validate_station_fields(
            &input.name,
            &input.station_type,
            &input.base_url,
            input.credit_per_cny,
        )?;

        let connection = self.connection()?;
        let existing_api_key: Option<String> = connection
            .query_row(
                "SELECT api_key FROM stations WHERE id = ?1",
                params![input.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("读取站点 API Key 失败: {error}"))?;

        let Some(existing_api_key) = existing_api_key else {
            return Err("站点不存在，无法更新".to_string());
        };

        let next_api_key = input
            .api_key
            .as_ref()
            .map(|api_key| api_key.trim().to_string())
            .filter(|api_key| !api_key.is_empty())
            .unwrap_or(existing_api_key);
        let now = now_string();

        connection
            .execute(
                "UPDATE stations
                    SET name = ?1,
                        station_type = ?2,
                        base_url = ?3,
                        api_key = ?4,
                        enabled = ?5,
                        credit_per_cny = ?6,
                        low_balance_threshold_cny = ?7,
                        status = CASE WHEN ?5 = 0 THEN 'disabled'
                                      WHEN status = 'disabled' THEN 'unchecked'
                                      ELSE status END,
                        note = ?8,
                        updated_at = ?9
                  WHERE id = ?10",
                params![
                    input.name.trim(),
                    input.station_type,
                    input.base_url.trim(),
                    next_api_key,
                    bool_to_i64(input.enabled),
                    input.credit_per_cny,
                    input.low_balance_threshold_cny,
                    normalize_optional_string(input.note),
                    now,
                    input.id,
                ],
            )
            .map_err(|error| format!("更新站点失败: {error}"))?;

        station_by_id(&connection, &input.id)
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

    pub fn update_station_key(&self, input: UpdateStationKeyInput) -> Result<StationKey, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &input.station_id)?;
        update_station_key_in_connection(&connection, input)
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

    pub fn proxy_rich_route_candidates(&self) -> Result<Vec<RichRouteCandidate>, String> {
        let connection = self.connection()?;
        proxy_rich_route_candidates_from_connection(&connection)
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

    pub fn get_station_login_password(
        &self,
        station_id: String,
    ) -> Result<Option<String>, String> {
        let connection = self.connection()?;
        validate_station_exists(&connection, &station_id)?;
        station_login_password_from_connection(&connection, &station_id)
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

    pub fn get_station_key_health(&self, station_key_id: String) -> Result<StationKeyHealth, String> {
        let connection = self.connection()?;
        station_key_health_by_id(&connection, &station_key_id)
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
        ",
    )
}

fn seed_default_settings(connection: &Connection) -> rusqlite::Result<()> {
    let defaults = [
        ("local_proxy_port", "8787"),
        ("local_key", "sk-local-pool-change-me"),
        ("default_routing_strategy", "manual"),
        ("low_balance_threshold_cny", "15"),
        ("collector_interval_minutes", "30"),
        ("tray_behavior", "minimize-to-tray"),
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

    Ok(Station {
        id: row.get(0)?,
        name: row.get(1)?,
        station_type: row.get(2)?,
        base_url: row.get(3)?,
        api_key_masked: mask_secret(&api_key),
        api_key_present: !api_key.is_empty(),
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
                    note, created_at, updated_at
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
                    note, created_at, updated_at
               FROM stations
              WHERE id = ?1",
            params![id],
            row_to_station,
        )
        .optional()
        .map_err(|error| format!("读取站点失败: {error}"))?
        .ok_or_else(|| "站点不存在".to_string())
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

fn validate_station_key_exists(connection: &Connection, station_key_id: &str) -> Result<(), String> {
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

    Ok(StationKey {
        id: row.get(0)?,
        station_id: row.get(1)?,
        name: row.get(2)?,
        api_key_masked: mask_secret(&api_key),
        api_key_present: !api_key.trim().is_empty(),
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
                    status, last_checked_at, last_used_at, note, created_at, updated_at
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

fn list_key_pool_items_from_connection(connection: &Connection) -> Result<Vec<KeyPoolItem>, String> {
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
            let supports_chat = i64_to_bool(row.get(17)?);
            let supports_responses = i64_to_bool(row.get(18)?);
            let supports_embeddings = i64_to_bool(row.get(19)?);
            let supports_stream = i64_to_bool(row.get(20)?);
            let supports_tools = i64_to_bool(row.get(21)?);
            let supports_vision = i64_to_bool(row.get(22)?);
            let supports_reasoning = i64_to_bool(row.get(23)?);
            let allowlist = parse_json_string_list(row.get::<_, String>(24)?.as_str());
            let blocklist = parse_json_string_list(row.get::<_, String>(25)?.as_str());
            let preferred_models = parse_json_string_list(row.get::<_, String>(26)?.as_str());
            let success_count = row.get::<_, Option<i64>>(29)?.unwrap_or(0);
            let failure_count = row.get::<_, Option<i64>>(30)?.unwrap_or(0);
            Ok(KeyPoolItem {
                id: row.get(0)?,
                station_id: row.get(1)?,
                station_name: row.get(2)?,
                station_type: row.get(3)?,
                station_base_url: row.get(4)?,
                name: row.get(5)?,
                api_key_masked: mask_secret(&api_key),
                api_key_present: !api_key.trim().is_empty(),
                enabled: i64_to_bool(row.get(7)?),
                priority: row.get(8)?,
                group_name: row.get(9)?,
                tier_label: row.get(10)?,
                status: row.get(11)?,
                last_checked_at: row.get(12)?,
                last_used_at: row.get(13)?,
                note: row.get(14)?,
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
                only_use_as_backup: i64_to_bool(row.get(27)?),
                cooldown_until: row.get(28)?,
                success_rate: success_rate(success_count, failure_count),
                avg_latency_ms: row.get(31)?,
                consecutive_failures: row.get(32)?,
                last_error_summary: row.get(33)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
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

fn proxy_route_candidates_from_connection(connection: &Connection) -> Result<Vec<RouteCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT k.id, k.station_id, s.base_url, k.api_key, s.upstream_api_format, k.priority
               FROM station_keys k
               JOIN stations s ON s.id = k.station_id
              WHERE k.enabled = 1
                AND s.enabled = 1
                AND TRIM(k.api_key) != ''
              ORDER BY k.priority ASC, k.created_at ASC",
        )
        .map_err(|error| format!("读取 Key 池候选失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(RouteCandidate {
                station_key_id: row.get(0)?,
                station_id: row.get(1)?,
                upstream_base_url: row.get(2)?,
                api_key: row.get(3)?,
                upstream_api_format: parse_upstream_api_format(row.get::<_, String>(4)?),
                priority: row.get(5)?,
            })
        })
        .map_err(|error| format!("查询 Key 池候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析 Key 池候选失败: {error}"))?;
    Ok(rows)
}

fn proxy_rich_route_candidates_from_connection(
    connection: &Connection,
) -> Result<Vec<RichRouteCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                k.id,
                k.station_id,
                s.base_url,
                k.api_key,
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
               AND TRIM(k.api_key) != ''
             ORDER BY k.priority ASC, k.created_at ASC",
        )
        .map_err(|error| format!("读取富路由候选失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            let station_key_id = row.get::<_, String>(0)?;
            let health_station_key_id = row.get::<_, Option<String>>(21)?;
            Ok(RichRouteCandidate {
                candidate: RouteCandidate {
                    station_key_id: station_key_id.clone(),
                    station_id: row.get(1)?,
                    upstream_base_url: row.get(2)?,
                    api_key: row.get(3)?,
                    upstream_api_format: parse_upstream_api_format(row.get::<_, String>(4)?),
                    priority: row.get(5)?,
                },
                station_name: row.get(6)?,
                key_name: row.get(7)?,
                capabilities: StationKeyCapabilities {
                    station_key_id,
                    supports_chat_completions: i64_to_bool(row.get(8)?),
                    supports_responses: i64_to_bool(row.get(9)?),
                    supports_embeddings: i64_to_bool(row.get(10)?),
                    supports_stream: i64_to_bool(row.get(11)?),
                    supports_tools: i64_to_bool(row.get(12)?),
                    supports_vision: i64_to_bool(row.get(13)?),
                    supports_reasoning: i64_to_bool(row.get(14)?),
                    model_allowlist: parse_json_string_list(row.get::<_, String>(15)?.as_str()),
                    model_blocklist: parse_json_string_list(row.get::<_, String>(16)?.as_str()),
                    preferred_models: parse_json_string_list(row.get::<_, String>(17)?.as_str()),
                    only_use_as_backup: i64_to_bool(row.get(18)?),
                    routing_tags: parse_json_string_list(row.get::<_, String>(19)?.as_str()),
                    updated_at: row.get(20)?,
                },
                health: health_station_key_id.map(|station_key_id| StationKeyHealth {
                    station_key_id,
                    last_success_at: row.get(22).ok().flatten(),
                    last_failure_at: row.get(23).ok().flatten(),
                    consecutive_failures: row.get(24).unwrap_or(0),
                    success_count: row.get(25).unwrap_or(0),
                    failure_count: row.get(26).unwrap_or(0),
                    avg_latency_ms: row.get(27).ok().flatten(),
                    last_error_summary: row.get(28).ok().flatten(),
                    cooldown_until: row.get(29).ok().flatten(),
                    updated_at: row.get(30).unwrap_or_else(|_| "0".to_string()),
                }),
            })
        })
        .map_err(|error| format!("查询富路由候选失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析富路由候选失败: {error}"))?;

    Ok(rows)
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

fn insert_request_log_in_connection(
    connection: &Connection,
    input: CreateRequestLogInput,
) -> Result<RequestLog, String> {
    let id = generate_id("request");
    let created_at = now_string();
    connection
        .execute(
            "INSERT INTO request_logs (
                id, started_at, finished_at, duration_ms, method, path, model, stream,
                status, station_key_id, station_id, upstream_base_url, fallback_count,
                error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                normalize_optional_string(input.error_message),
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
        created_at: row.get(14)?,
    })
}

fn request_log_by_id(connection: &Connection, id: &str) -> Result<RequestLog, String> {
    connection
        .query_row(
            "SELECT id, started_at, finished_at, duration_ms, method, path, model, stream,
                    status, station_key_id, station_id, upstream_base_url, fallback_count,
                    error_message, created_at
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
                    error_message, created_at
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
    validate_station_exists(connection, &input.station_id)?;
    if input.name.trim().is_empty() {
        return Err("Key 名称不能为空".to_string());
    }
    if input.api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }

    let id = generate_id("key");
    let now = now_string();
    let priority = match input.priority {
        Some(priority) => priority,
        None => next_station_key_priority(connection, &input.station_id)?,
    };

    connection
        .execute(
            "INSERT INTO station_keys (
                id, station_id, name, api_key, enabled, priority, group_name, tier_label,
                status, last_checked_at, last_used_at, note, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'unchecked', NULL, NULL, ?9, ?10, ?11)",
            params![
                id,
                input.station_id,
                input.name.trim(),
                input.api_key.trim(),
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
    if input.name.trim().is_empty() {
        return Err("Key 名称不能为空".to_string());
    }

    let existing_api_key: Option<String> = connection
        .query_row(
            "SELECT api_key FROM station_keys WHERE id = ?1 AND station_id = ?2",
            params![input.id, input.station_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 Station Key 失败: {error}"))?;

    let Some(existing_api_key) = existing_api_key else {
        return Err("Station Key 不存在，无法更新".to_string());
    };

    let next_api_key = input
        .api_key
        .as_ref()
        .map(|api_key| api_key.trim().to_string())
        .filter(|api_key| !api_key.is_empty())
        .unwrap_or(existing_api_key);
    let now = now_string();

    connection
        .execute(
            "UPDATE station_keys
                SET name = ?1,
                    api_key = ?2,
                    enabled = ?3,
                    priority = ?4,
                    group_name = ?5,
                    tier_label = ?6,
                    status = ?7,
                    note = ?8,
                    updated_at = ?9
              WHERE id = ?10 AND station_id = ?11",
            params![
                input.name.trim(),
                next_api_key,
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
                    status, last_checked_at, last_used_at, note, created_at, updated_at
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
            "SELECT station_id, login_username, login_password, remember_password,
                    login_status, login_error, last_login_at, session_status,
                    session_expires_at, updated_at
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| {
                let password: Option<String> = row.get(2)?;
                Ok(StationCredentials {
                    station_id: row.get(0)?,
                    login_username: row.get(1)?,
                    password_present: password
                        .map(|value| !value.trim().is_empty())
                        .unwrap_or(false),
                    remember_password: i64_to_bool(row.get(3)?),
                    login_status: row.get(4)?,
                    login_error: row.get(5)?,
                    last_login_at: row.get(6)?,
                    session_status: row.get(7)?,
                    session_expires_at: row.get(8)?,
                    updated_at: row.get(9)?,
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
    connection
        .query_row(
            "SELECT login_password
               FROM station_credentials
              WHERE station_id = ?1",
            params![station_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("读取登录密码失败: {error}"))?
        .map(|password| {
            password.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
        })
        .ok_or_else(|| "未找到登录信息".to_string())
}

fn upsert_station_credentials(
    connection: &Connection,
    input: UpdateStationCredentialsInput,
) -> Result<(), String> {
    let existing_password: Option<String> = connection
        .query_row(
            "SELECT login_password FROM station_credentials WHERE station_id = ?1",
            params![input.station_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取旧密码失败: {error}"))?
        .flatten();

    let password = if input.remember_password {
        input
            .login_password
            .as_ref()
            .map(|password| password.trim().to_string())
            .filter(|password| !password.is_empty())
            .or(existing_password)
    } else {
        None
    };
    let now = now_string();

    connection
        .execute(
            "INSERT INTO station_credentials (
                id, station_id, login_username, login_password, remember_password,
                login_status, login_error, last_login_at, session_status,
                session_expires_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'saved', NULL, NULL, 'none', NULL, ?6, ?7)
             ON CONFLICT(station_id) DO UPDATE SET
                login_username = excluded.login_username,
                login_password = excluded.login_password,
                remember_password = excluded.remember_password,
                login_status = 'saved',
                login_error = NULL,
                updated_at = excluded.updated_at",
            params![
                generate_id("credentials"),
                input.station_id,
                normalize_optional_string(input.login_username),
                password,
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
                normalize_optional_string(error_message),
                now,
            ],
        )
        .map_err(|error| format!("保存采集快照失败: {error}"))?;

    collector_snapshot_by_id(connection, &id)
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
    format!("{prefix}-{}", now_millis())
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
        let loaded = database
            .get_station_key_capabilities(key.id)
            .expect("load");

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
}
