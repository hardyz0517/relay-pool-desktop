use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::{
        proxy::{normalize_proxy_mode, normalize_proxy_url},
        routing::{RoutingGroupFilter, SchedulerAdvancedSettings},
        secrets::mask_secret,
        settings::{
            AppSettings, ConfirmHierarchicalRoutingMigrationInput,
            HierarchicalRoutingMigrationConfig, UpdateSettingsInput,
        },
    },
    persistence::{
        error::PersistenceError,
        read_session::ReadSession,
        settings_compat::canonical_tray_behavior,
        stores::credential_store::{
            EncryptedSecretRow, StoredEncryptedSecret as CredentialStoredEncryptedSecret,
        },
        write_session::WriteSession,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SettingsStore;

#[derive(Debug, Clone)]
pub(crate) struct SettingsUpdate {
    pub(crate) now: String,
    pub(crate) input: UpdateSettingsInput,
}

impl SettingsStore {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn load(
        &self,
        read: &mut ReadSession,
        data_dir: &str,
        pending_data_dir: Option<String>,
    ) -> Result<AppSettings, PersistenceError> {
        settings_from_connection(read.connection(), data_dir, pending_data_dir).await
    }

    pub(crate) async fn local_access_key_secret(
        &self,
        read: &mut ReadSession,
    ) -> Result<Option<CredentialStoredEncryptedSecret>, PersistenceError> {
        local_access_key_secret_from_connection(read.connection()).await
    }

    pub(crate) async fn local_access_key_secret_for_write(
        &self,
        write: &mut WriteSession,
    ) -> Result<Option<CredentialStoredEncryptedSecret>, PersistenceError> {
        local_access_key_secret_from_connection(write.connection()).await
    }

    pub(crate) async fn local_access_key_setting_value(
        &self,
        write: &mut WriteSession,
    ) -> Result<String, PersistenceError> {
        read_setting(write.connection(), "local_key").await
    }

    pub(crate) async fn upsert_local_access_key_secret(
        &self,
        write: &mut WriteSession,
        secret: &EncryptedSecretRow,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO secrets (
                id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                key_id, encryption_version, value_hash, created_at, updated_at
            ) VALUES (?1, 'settings', 'local_key', 'local_access_key', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(scope, owner_id, kind) DO UPDATE SET
                masked_value = excluded.masked_value,
                ciphertext = excluded.ciphertext,
                nonce = excluded.nonce,
                key_id = excluded.key_id,
                encryption_version = excluded.encryption_version,
                value_hash = excluded.value_hash,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&secret.id)
        .bind(&secret.masked_value)
        .bind(&secret.ciphertext)
        .bind(&secret.nonce)
        .bind(&secret.key_id)
        .bind(i64::from(secret.encryption_version))
        .bind(&secret.value_hash)
        .bind(&secret.now)
        .execute(write.connection())
        .await?;
        let secret_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM secrets WHERE scope = 'settings' AND owner_id = 'local_key' AND kind = 'local_access_key'",
        )
        .fetch_one(write.connection())
        .await?;
        sqlx::query(
            r#"
            INSERT INTO app_secret_bindings (
                binding_scope, binding_owner_id, binding_kind, secret_id, created_at, updated_at
            ) VALUES ('settings', 'local_key', 'local_access_key', ?1, ?2, ?2)
            ON CONFLICT(binding_scope, binding_owner_id, binding_kind) DO UPDATE SET
                secret_id = excluded.secret_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(secret_id)
        .bind(&secret.now)
        .execute(write.connection())
        .await?;
        sqlx::query("UPDATE settings SET value = '', updated_at = ?1 WHERE key = 'local_key'")
            .bind(&secret.now)
            .execute(write.connection())
            .await?;
        Ok(())
    }

    pub(crate) async fn update_local_access_key(
        &self,
        write: &mut WriteSession,
        value: &str,
        now: &str,
        data_dir: &str,
        pending_data_dir: Option<String>,
    ) -> Result<AppSettings, PersistenceError> {
        let local_key = value.trim();
        if local_key.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        upsert_setting(write.connection(), "local_key", local_key, now).await?;
        settings_from_connection(write.connection(), data_dir, pending_data_dir).await
    }

    pub(crate) async fn set_local_proxy_start_on_launch(
        &self,
        write: &mut WriteSession,
        enabled: bool,
        now: &str,
    ) -> Result<(), PersistenceError> {
        upsert_setting(
            write.connection(),
            "local_proxy_start_on_launch",
            &enabled.to_string(),
            now,
        )
        .await
    }

    pub(crate) async fn update(
        &self,
        write: &mut WriteSession,
        update: SettingsUpdate,
        data_dir: &str,
        pending_data_dir: Option<String>,
    ) -> Result<AppSettings, PersistenceError> {
        validate_settings(&update.input)?;
        let current =
            settings_from_connection(write.connection(), data_dir, pending_data_dir.clone())
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
                return Err(PersistenceError::ConstraintViolation);
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
            .map_err(|_| PersistenceError::ConstraintViolation)?;
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
            .map_err(|_| setting_serialization_failed())?;
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
        settings_from_connection(write.connection(), data_dir, pending_data_dir).await
    }

    pub(crate) async fn confirm_hierarchical_routing_migration(
        &self,
        write: &mut WriteSession,
        input: ConfirmHierarchicalRoutingMigrationInput,
        confirmed_at_ms: i64,
        now: &str,
        data_dir: &str,
        pending_data_dir: Option<String>,
    ) -> Result<AppSettings, PersistenceError> {
        validate_hierarchical_routing_migration(&input)?;
        let config = HierarchicalRoutingMigrationConfig {
            config_version: "hierarchical_routing_migration_v1".to_string(),
            policy_version: "hierarchical_v1".to_string(),
            ordering_profile: input.ordering_profile,
            multiplier_ceiling: input.multiplier_ceiling,
            group_scope: input.group_scope,
            allow_depleted_fallback: input.allow_depleted_fallback,
            affinity_mode: input.affinity_mode,
            legacy_policy: input.legacy_policy,
            confirmed_at_ms,
        };
        let value = serde_json::to_string(&config).map_err(|_| setting_serialization_failed())?;
        upsert_setting(
            write.connection(),
            "hierarchical_routing_migration_v1_json",
            &value,
            now,
        )
        .await?;
        settings_from_connection(write.connection(), data_dir, pending_data_dir).await
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "upgrade integration targets import allowlisted settings through the application service"
        )
    )]
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
    let local_key_masked = local_access_key_masked(&mut *connection).await?;
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
        local_key_masked,
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
        hierarchical_routing_migration: parse_hierarchical_routing_migration(
            &read_setting_or_default(
                &mut *connection,
                "hierarchical_routing_migration_v1_json",
                "",
            )
            .await?,
        )?,
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

async fn local_access_key_masked(
    connection: &mut SqliteConnection,
) -> Result<String, PersistenceError> {
    if let Some(secret) = local_access_key_secret_from_connection(&mut *connection).await? {
        return Ok(secret.masked_value);
    }
    let local_key = read_setting(&mut *connection, "local_key").await?;
    Ok(mask_secret(&local_key))
}

async fn local_access_key_secret_from_connection(
    connection: &mut SqliteConnection,
) -> Result<Option<CredentialStoredEncryptedSecret>, PersistenceError> {
    let has_binding_table = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'app_secret_bindings')",
    )
    .fetch_one(&mut *connection)
    .await?;
    if has_binding_table != 1 {
        return Ok(None);
    }
    let row = sqlx::query(
        r#"
        SELECT s.id, s.scope, s.owner_id, s.kind, s.masked_value, s.ciphertext, s.nonce,
               s.key_id, s.encryption_version, s.value_hash
        FROM app_secret_bindings b
        JOIN secrets s ON s.id = b.secret_id
        WHERE b.binding_scope = 'settings'
          AND b.binding_owner_id = 'local_key'
          AND b.binding_kind = 'local_access_key'
          AND s.scope = 'settings'
          AND s.owner_id = 'local_key'
          AND s.kind = 'local_access_key'
        "#,
    )
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| CredentialStoredEncryptedSecret {
        id: row.get("id"),
        scope: row.get("scope"),
        owner_id: row.get("owner_id"),
        kind: row.get("kind"),
        masked_value: row.get("masked_value"),
        ciphertext: row.get("ciphertext"),
        nonce: row.get("nonce"),
        key_id: row.get("key_id"),
        encryption_version: row.get::<i64, _>("encryption_version") as u16,
        value_hash: row.get("value_hash"),
    }))
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
        .ok_or(PersistenceError::NotFound)
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
        .map_err(|_| invalid_persisted_setting())
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
        .map_err(|_| invalid_persisted_setting())
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
        .map_err(|_| invalid_persisted_setting())?;
    if !parsed.is_finite() {
        return Err(invalid_persisted_setting());
    }
    Ok(Some(parsed))
}

fn serialize_routing_group_filter_setting(
    filter: &RoutingGroupFilter,
) -> Result<String, PersistenceError> {
    match serde_json::to_value(filter).map_err(|_| setting_serialization_failed())? {
        serde_json::Value::String(value) => Ok(value),
        value => serde_json::to_string(&value).map_err(|_| setting_serialization_failed()),
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
        .map_err(|_| invalid_persisted_setting())
}

fn parse_scheduler_advanced_settings(
    value: &str,
) -> Result<SchedulerAdvancedSettings, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(SchedulerAdvancedSettings::default());
    }
    let settings: SchedulerAdvancedSettings =
        serde_json::from_str(value).map_err(|_| invalid_persisted_setting())?;
    settings
        .validate()
        .map_err(|_| invalid_persisted_setting())?;
    Ok(settings)
}

fn parse_hierarchical_routing_migration(
    value: &str,
) -> Result<Option<HierarchicalRoutingMigrationConfig>, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let config: HierarchicalRoutingMigrationConfig =
        serde_json::from_str(value).map_err(|_| invalid_persisted_setting())?;
    if config.config_version != "hierarchical_routing_migration_v1"
        || config.policy_version != "hierarchical_v1"
        || !config.multiplier_ceiling.is_finite()
        || config.multiplier_ceiling < 0.0
    {
        return Err(invalid_persisted_setting());
    }
    Ok(Some(config))
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
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_hierarchical_routing_migration(
    input: &ConfirmHierarchicalRoutingMigrationInput,
) -> Result<(), PersistenceError> {
    if !input.multiplier_ceiling.is_finite() || input.multiplier_ceiling < 0.0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    if !matches!(
        input.legacy_policy.as_str(),
        "automatic_balanced"
            | "priority_fallback"
            | "stable_first"
            | "backup_only"
            | "cheap_first"
            | "cost_stable_first"
    ) {
        return Err(PersistenceError::ConstraintViolation);
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
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(normalized)
}

fn validate_tray_behavior_setting(value: &str) -> Result<String, PersistenceError> {
    canonical_tray_behavior(value)
        .map(str::to_string)
        .ok_or(PersistenceError::ConstraintViolation)
}

fn invalid_persisted_setting() -> PersistenceError {
    PersistenceError::InvariantViolation("invalid persisted setting".to_string())
}

fn setting_serialization_failed() -> PersistenceError {
    PersistenceError::InvariantViolation("setting serialization failed".to_string())
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "upgrade integration targets include the settings store without every importer consumer"
    )
)]
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
            | "hierarchical_routing_migration_v1_json"
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
