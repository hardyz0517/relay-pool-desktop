use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::{
        proxy::{normalize_proxy_mode, normalize_proxy_url},
        routing::{DispatchAlgorithmSettings, RoutingGroupFilter},
        routing_policy::RoutingPolicyConfigV1,
        secrets::mask_secret,
        settings::{AppSettings, UpdateSettingsInput},
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
        let tray_behavior = validate_tray_behavior_setting(
            update
                .input
                .tray_behavior
                .as_deref()
                .unwrap_or(&current.tray_behavior),
        )?;
        let scheduler_config = serde_json::to_string(
            &update
                .input
                .scheduler_config
                .clone()
                .unwrap_or(current.scheduler_config.clone()),
        )
        .map_err(|_| PersistenceError::ConstraintViolation)?;

        let values = [
            (
                "local_proxy_port",
                update.input.local_proxy_port.to_string(),
            ),
            ("collector_proxy_mode", collector_proxy_mode),
            (
                "collector_proxy_url",
                collector_proxy_url.unwrap_or_default(),
            ),
            ("scheduler_advanced_settings_json", scheduler_config),
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

    let canonical_policy = canonical_policy_projection(&mut *connection).await?;

    Ok(AppSettings {
        local_proxy_port: parse_setting(&mut *connection, "local_proxy_port").await?,
        local_proxy_start_on_launch: parse_setting_or_default(
            &mut *connection,
            "local_proxy_start_on_launch",
            "false",
        )
        .await?,
        local_key_masked,
        // The UI field is a compatibility projection. Runtime routing reads
        // the versioned routing_policy aggregate directly.
        routing_policy_name: "automatic_balanced".to_string(),
        collector_proxy_mode: normalize_proxy_mode(
            &read_setting_or_default(&mut *connection, "collector_proxy_mode", "direct").await?,
            false,
        ),
        collector_proxy_url: normalize_proxy_url(Some(
            read_setting_or_default(&mut *connection, "collector_proxy_url", "").await?,
        )),
        max_rate_multiplier: canonical_policy.max_rate_multiplier,
        routing_group_scope: canonical_policy.routing_group_filter,
        scheduler_config: parse_scheduler_settings(
            &mut *connection,
            "scheduler_advanced_settings_json",
        )
        .await?,
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
        allow_depleted_fallback: canonical_policy.allow_depleted_fallback,
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

#[derive(Debug, Clone)]
struct CanonicalPolicyProjection {
    allow_depleted_fallback: bool,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
}

async fn canonical_policy_projection(
    connection: &mut SqliteConnection,
) -> Result<CanonicalPolicyProjection, PersistenceError> {
    let config_json = sqlx::query_scalar::<_, String>(
        "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
    )
    .fetch_optional(&mut *connection)
    .await?;
    let Some(config_json) = config_json else {
        return Ok(CanonicalPolicyProjection {
            allow_depleted_fallback: false,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::AllGroups,
        });
    };
    let config = serde_json::from_str::<RoutingPolicyConfigV1>(&config_json)
        .map_err(|_| invalid_persisted_setting())?;
    config.validate().map_err(|_| invalid_persisted_setting())?;
    Ok(CanonicalPolicyProjection {
        allow_depleted_fallback: config.allow_depleted_fallback,
        max_rate_multiplier: config.max_rate_multiplier,
        routing_group_filter: config.routing_group_filter,
    })
}

fn validate_settings(input: &UpdateSettingsInput) -> Result<(), PersistenceError> {
    if input.local_proxy_port == 0
        || input.low_balance_threshold_cny < 0.0
        || input.collector_interval_minutes == 0
        || input.balance_interval_minutes == 0
        || input.group_rate_interval_minutes == 0
        || input.pricing_refresh_interval_minutes == 0
        || input.collector_timeout_seconds < 3
        || input.collector_max_concurrency == 0
        || input.collector_max_concurrency > 8
    {
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

async fn parse_scheduler_settings(
    connection: &mut SqliteConnection,
    key: &str,
) -> Result<DispatchAlgorithmSettings, PersistenceError> {
    let value = read_setting_or_default(&mut *connection, key, "").await?;
    if value.trim().is_empty() {
        return Ok(DispatchAlgorithmSettings::default());
    }
    let settings: DispatchAlgorithmSettings =
        serde_json::from_str(&value).map_err(|_| invalid_persisted_setting())?;
    settings
        .validate()
        .map_err(|_| invalid_persisted_setting())?;
    Ok(settings)
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
            | "collector_proxy_mode"
            | "collector_proxy_url"
            | "max_rate_multiplier"
            | "default_routing_group_filter"
            | "scheduler_advanced_settings_json"
            | "low_balance_threshold_cny"
            | "collector_interval_minutes"
            | "balance_interval_minutes"
            | "group_rate_interval_minutes"
            | "pricing_refresh_interval_minutes"
            | "collector_timeout_seconds"
            | "collector_max_concurrency"
            | "developer_mode_enabled"
            | "tray_behavior"
    )
}
