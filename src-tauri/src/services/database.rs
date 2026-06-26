use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

use crate::models::{
    settings::{AppSettings, UpdateSettingsInput},
    stations::{CreateStationInput, Station, UpdateStationInput},
};

pub struct AppDatabase {
    connection: Mutex<Connection>,
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

        Ok(Self {
            connection: Mutex::new(connection),
            db_path,
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
        enabled: i64_to_bool(row.get(5)?),
        priority: row.get(6)?,
        credit_per_cny: row.get(7)?,
        balance_raw: row.get(8)?,
        balance_cny: row.get(9)?,
        low_balance_threshold_cny: row.get(10)?,
        status: row.get(11)?,
        latency_ms: row.get(12)?,
        last_checked_at: row.get(13)?,
        last_pricing_fetched_at: row.get(14)?,
        note: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn list_stations_from_connection(connection: &Connection) -> Result<Vec<Station>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, station_type, base_url, api_key, enabled, priority,
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
            "SELECT id, name, station_type, base_url, api_key, enabled, priority,
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

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
