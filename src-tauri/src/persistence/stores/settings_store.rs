use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::{
        routing::{RoutingGroupFilter, SchedulerAdvancedSettings},
        settings::{AppSettings, UpdateSettingsInput},
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
    services::{
        outbound::{normalize_proxy_mode, normalize_proxy_url},
        secrets::mask::mask_secret,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct SettingsStore {
    data_dir: String,
    pending_data_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsUpdate {
    pub(crate) now: String,
    pub(crate) input: UpdateSettingsInput,
}

impl SettingsStore {
    pub(crate) fn new(data_dir: String, pending_data_dir: Option<String>) -> Self {
        Self {
            data_dir,
            pending_data_dir,
        }
    }

    pub(crate) async fn load(
        &self,
        read: &mut ReadSession,
    ) -> Result<AppSettings, PersistenceError> {
        settings_from_connection(
            read.connection(),
            &self.data_dir,
            self.pending_data_dir.clone(),
        )
        .await
    }

    pub(crate) async fn update_local_access_key(
        &self,
        write: &mut WriteSession,
        value: &str,
        now: &str,
    ) -> Result<AppSettings, PersistenceError> {
        let local_key = value.trim();
        if local_key.is_empty() {
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        upsert_setting(write.connection(), "local_key", local_key, now).await?;
        settings_from_connection(
            write.connection(),
            &self.data_dir,
            self.pending_data_dir.clone(),
        )
        .await
    }

    pub(crate) async fn update(
        &self,
        write: &mut WriteSession,
        update: SettingsUpdate,
    ) -> Result<AppSettings, PersistenceError> {
        validate_settings(&update.input)?;
        let current = settings_from_connection(
            write.connection(),
            &self.data_dir,
            self.pending_data_dir.clone(),
        )
        .await?;
        let collector_proxy_mode = validate_proxy_config(
            &update.input.collector_proxy_mode,
            update.input.collector_proxy_url.clone(),
            false,
        )?;
        let collector_proxy_url = normalize_proxy_url(update.input.collector_proxy_url.clone());
        let max_rate_multiplier = update
            .input
            .max_rate_multiplier
            .unwrap_or(current.max_rate_multiplier);
        if let Some(value) = max_rate_multiplier {
            if !value.is_finite() || value < 0.0 {
                return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
            }
        }
        let default_routing_group_filter = update
            .input
            .default_routing_group_filter
            .unwrap_or(current.default_routing_group_filter);
        let scheduler_advanced_settings = update
            .input
            .scheduler_advanced_settings
            .unwrap_or(current.scheduler_advanced_settings);
        scheduler_advanced_settings
            .validate()
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
        let tray_behavior = validate_tray_behavior_setting(
            update
                .input
                .tray_behavior
                .as_deref()
                .unwrap_or(&current.tray_behavior),
        )?;

        let default_routing_group_filter =
            serialize_routing_group_filter_setting(&default_routing_group_filter)?;
        let scheduler_advanced_settings = serde_json::to_string(&scheduler_advanced_settings)
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
        let values = [
            (
                "local_proxy_port",
                update.input.local_proxy_port.to_string(),
            ),
            (
                "default_routing_strategy",
                update.input.default_routing_strategy,
            ),
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
                update.input.low_balance_threshold_cny.to_string(),
            ),
            (
                "collector_interval_minutes",
                update.input.collector_interval_minutes.to_string(),
            ),
            (
                "balance_interval_minutes",
                update.input.balance_interval_minutes.to_string(),
            ),
            (
                "group_rate_interval_minutes",
                update.input.group_rate_interval_minutes.to_string(),
            ),
            (
                "model_list_interval_minutes",
                update.input.model_list_interval_minutes.to_string(),
            ),
            (
                "pricing_refresh_interval_minutes",
                update.input.pricing_refresh_interval_minutes.to_string(),
            ),
            (
                "collector_timeout_seconds",
                update.input.collector_timeout_seconds.to_string(),
            ),
            (
                "collector_max_concurrency",
                update.input.collector_max_concurrency.to_string(),
            ),
            (
                "allow_depleted_fallback",
                update.input.allow_depleted_fallback.to_string(),
            ),
            (
                "developer_mode_enabled",
                update.input.developer_mode_enabled.to_string(),
            ),
            ("tray_behavior", tray_behavior),
        ];
        for (key, value) in values {
            upsert_setting(write.connection(), key, &value, &update.now).await?;
        }
        settings_from_connection(
            write.connection(),
            &self.data_dir,
            self.pending_data_dir.clone(),
        )
        .await
    }

    pub(crate) async fn import_known_legacy_settings(
        &self,
        write: &mut WriteSession,
        values: &[(String, String)],
        now: &str,
    ) -> Result<(), PersistenceError> {
        for (key, value) in values {
            if is_supported_setting_key(key) {
                upsert_setting(write.connection(), key, value, now).await?;
            }
        }
        Ok(())
    }
}

async fn settings_from_connection(
    connection: &mut SqliteConnection,
    data_dir: &str,
    pending_data_dir: Option<String>,
) -> Result<AppSettings, PersistenceError> {
    let local_key = read_setting(&mut *connection, "local_key").await?;
    let data_dir_change_requires_restart = pending_data_dir
        .as_ref()
        .map(|pending| pending != data_dir)
        .unwrap_or(false);

    Ok(AppSettings {
        local_proxy_port: parse_setting(&mut *connection, "local_proxy_port").await?,
        local_proxy_start_on_launch: parse_setting_or_default(
            &mut *connection,
            "local_proxy_start_on_launch",
            "false",
        )
        .await?,
        local_key_masked: mask_secret(&local_key),
        default_routing_strategy: read_setting(&mut *connection, "default_routing_strategy")
            .await?,
        collector_proxy_mode: normalize_proxy_mode(
            &read_setting_or_default(&mut *connection, "collector_proxy_mode", "direct").await?,
            false,
        ),
        collector_proxy_url: normalize_proxy_url(Some(
            read_setting_or_default(&mut *connection, "collector_proxy_url", "").await?,
        )),
        max_rate_multiplier: parse_optional_f64_setting(
            &read_setting_or_default(&mut *connection, "max_rate_multiplier", "").await?,
        )?,
        default_routing_group_filter: parse_routing_group_filter_setting(
            &read_setting_or_default(
                &mut *connection,
                "default_routing_group_filter",
                "all_groups",
            )
            .await?,
        )?,
        scheduler_advanced_settings: parse_scheduler_advanced_settings(
            &read_setting_or_default(&mut *connection, "scheduler_advanced_settings_json", "")
                .await?,
        )?,
        low_balance_threshold_cny: parse_setting(&mut *connection, "low_balance_threshold_cny")
            .await?,
        collector_interval_minutes: parse_setting(&mut *connection, "collector_interval_minutes")
            .await?,
        balance_interval_minutes: parse_setting_or_default(
            &mut *connection,
            "balance_interval_minutes",
            "5",
        )
        .await?,
        group_rate_interval_minutes: parse_setting_or_default(
            &mut *connection,
            "group_rate_interval_minutes",
            "20",
        )
        .await?,
        model_list_interval_minutes: parse_setting_or_default(
            &mut *connection,
            "model_list_interval_minutes",
            "60",
        )
        .await?,
        pricing_refresh_interval_minutes: parse_setting_or_default(
            &mut *connection,
            "pricing_refresh_interval_minutes",
            "60",
        )
        .await?,
        collector_timeout_seconds: parse_setting_or_default(
            &mut *connection,
            "collector_timeout_seconds",
            "15",
        )
        .await?,
        collector_max_concurrency: parse_setting_or_default(
            &mut *connection,
            "collector_max_concurrency",
            "3",
        )
        .await?,
        allow_depleted_fallback: parse_setting_or_default(
            &mut *connection,
            "allow_depleted_fallback",
            "false",
        )
        .await?,
        developer_mode_enabled: parse_setting_or_default(
            &mut *connection,
            "developer_mode_enabled",
            "false",
        )
        .await?,
        tray_behavior: validate_tray_behavior_setting(
            &read_setting_or_default(&mut *connection, "tray_behavior", "close_to_tray").await?,
        )?,
        data_dir: data_dir.to_string(),
        pending_data_dir,
        data_dir_change_requires_restart,
    })
}

async fn read_setting<'e, E>(executor: E, key: &str) -> Result<String, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(executor)
        .await?
        .map(|row| row.get("value"))
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn read_setting_or_default<'e, E>(
    executor: E,
    key: &str,
    default_value: &str,
) -> Result<String, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    Ok(sqlx::query("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(executor)
        .await?
        .map(|row| row.get("value"))
        .unwrap_or_else(|| default_value.to_string()))
}

async fn parse_setting<'e, E, T>(executor: E, key: &str) -> Result<T, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
    T: std::str::FromStr,
{
    read_setting(executor, key)
        .await?
        .parse()
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn parse_setting_or_default<'e, E, T>(
    executor: E,
    key: &str,
    default_value: &str,
) -> Result<T, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
    T: std::str::FromStr,
{
    read_setting_or_default(executor, key, default_value)
        .await?
        .parse()
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn upsert_setting<'e, E>(
    executor: E,
    key: &str,
    value: &str,
    now: &str,
) -> Result<(), PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(executor)
    .await?;
    Ok(())
}

fn parse_optional_f64_setting(value: &str) -> Result<Option<f64>, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    if !parsed.is_finite() {
        return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
    }
    Ok(Some(parsed))
}

fn serialize_routing_group_filter_setting(
    filter: &RoutingGroupFilter,
) -> Result<String, PersistenceError> {
    match serde_json::to_value(filter)
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?
    {
        serde_json::Value::String(value) => Ok(value),
        value => serde_json::to_string(&value)
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound)),
    }
}

fn parse_routing_group_filter_setting(value: &str) -> Result<RoutingGroupFilter, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(RoutingGroupFilter::AllGroups);
    }
    serde_json::from_str::<RoutingGroupFilter>(value)
        .or_else(|_| {
            serde_json::from_value::<RoutingGroupFilter>(serde_json::Value::String(
                value.to_string(),
            ))
        })
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

fn parse_scheduler_advanced_settings(
    value: &str,
) -> Result<SchedulerAdvancedSettings, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(SchedulerAdvancedSettings::default());
    }
    let settings: SchedulerAdvancedSettings = serde_json::from_str(value)
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    settings
        .validate()
        .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    Ok(settings)
}

fn validate_settings(input: &UpdateSettingsInput) -> Result<(), PersistenceError> {
    if input.local_proxy_port == 0
        || input.low_balance_threshold_cny < 0.0
        || input.collector_interval_minutes == 0
        || input.balance_interval_minutes == 0
        || input.group_rate_interval_minutes == 0
        || input.model_list_interval_minutes == 0
        || input.pricing_refresh_interval_minutes == 0
        || input.collector_timeout_seconds < 3
        || input.collector_max_concurrency == 0
        || input.collector_max_concurrency > 8
    {
        return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
    }
    Ok(())
}

fn validate_proxy_config(
    mode: &str,
    url: Option<String>,
    allow_inherit: bool,
) -> Result<String, PersistenceError> {
    let normalized = normalize_proxy_mode(mode, allow_inherit);
    let proxy_url = normalize_proxy_url(url);
    if normalized == "manual" && proxy_url.is_none() {
        return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
    }
    Ok(normalized)
}

fn validate_tray_behavior_setting(value: &str) -> Result<String, PersistenceError> {
    match value {
        "minimize_to_tray" | "close_to_tray" | "disabled" => Ok(value.to_string()),
        _ => Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound)),
    }
}

fn is_supported_setting_key(key: &str) -> bool {
    matches!(
        key,
        "local_proxy_port"
            | "local_proxy_start_on_launch"
            | "local_key"
            | "default_routing_strategy"
            | "collector_proxy_mode"
            | "collector_proxy_url"
            | "max_rate_multiplier"
            | "default_routing_group_filter"
            | "scheduler_advanced_settings_json"
            | "low_balance_threshold_cny"
            | "collector_interval_minutes"
            | "balance_interval_minutes"
            | "group_rate_interval_minutes"
            | "model_list_interval_minutes"
            | "pricing_refresh_interval_minutes"
            | "collector_timeout_seconds"
            | "collector_max_concurrency"
            | "allow_depleted_fallback"
            | "developer_mode_enabled"
            | "tray_behavior"
    )
}
