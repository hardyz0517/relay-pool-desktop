use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use futures_util::TryStreamExt;
use sqlx::{Column, Row, SqliteConnection, TypeInfo, ValueRef};

use crate::persistence::{runtime::PersistenceHandle, write_session::WriteSession};

use super::{DetectedLegacyProfile, LegacyReadSession, LegacySecretBytes, UpgradeError};

pub(crate) enum LegacySecretMaterial {
    Plaintext {
        scope: String,
        owner_id: String,
        kind: String,
        value: LegacySecretBytes,
    },
    EncryptedV1 {
        scope: String,
        owner_id: String,
        kind: String,
        ciphertext: LegacySecretBytes,
        nonce: LegacySecretBytes,
        aad: String,
    },
}

pub(crate) struct ImportedEncryptedSecret {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) owner_id: String,
    pub(crate) kind: String,
    pub(crate) masked_value: String,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) trait LegacySecretTransformer: Send + Sync {
    fn transform(
        &self,
        profile_id: &str,
        material: LegacySecretMaterial,
    ) -> Result<ImportedEncryptedSecret, UpgradeError>;
}

#[derive(Clone)]
enum LegacyValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Clone)]
struct ImportedRow(BTreeMap<String, LegacyValue>);

pub(crate) async fn import_profile(
    profile: &DetectedLegacyProfile,
    source_path: &Path,
    target: &PersistenceHandle,
) -> Result<(), UpgradeError> {
    let mut source = verified_session(profile, source_path).await?;
    let result = (profile.descriptor.import)(&mut source, target).await;
    source.close().await?;
    result
}

pub(crate) async fn import_profile_with_secrets(
    profile: &DetectedLegacyProfile,
    source_path: &Path,
    target: &PersistenceHandle,
    transformer: &dyn LegacySecretTransformer,
) -> Result<(), UpgradeError> {
    let mut source = verified_session(profile, source_path).await?;
    let result = import_additive_v1(profile.id(), &mut source, target, Some(transformer)).await;
    source.close().await?;
    result
}

async fn verified_session(
    profile: &DetectedLegacyProfile,
    source_path: &Path,
) -> Result<LegacyReadSession, UpgradeError> {
    let mut source = LegacyReadSession::open(source_path).await?;
    if source.schema_hash().await? != profile.schema_hash() {
        source.close().await?;
        return Err(UpgradeError::UnsupportedLegacySchema);
    }
    Ok(source)
}

pub(super) async fn import_additive_v1(
    profile_id: &'static str,
    source: &mut LegacyReadSession,
    target: &PersistenceHandle,
    transformer: Option<&dyn LegacySecretTransformer>,
) -> Result<(), UpgradeError> {
    let mut write = target.begin_write().await?;
    ensure_empty_target(&mut write).await?;
    for plan in TABLE_PLANS {
        copy_table(profile_id, source.connection(), &mut write, plan).await?;
    }
    for secret in load_secrets(profile_id, source.connection(), transformer).await? {
        insert_secret(&mut write, secret).await?;
    }
    write.commit().await?;
    Ok(())
}

async fn ensure_empty_target(
    write: &mut WriteSession,
) -> Result<(), crate::persistence::error::PersistenceError> {
    for table in ["stations", "station_keys", "request_logs", "collector_runs"] {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = sqlx::query_scalar(&sql)
            .fetch_one(write.connection())
            .await?;
        if count != 0 {
            return Err(
                crate::persistence::error::PersistenceError::InvariantViolation(
                    "legacy import target is not empty".to_string(),
                ),
            );
        }
    }
    Ok(())
}

async fn copy_table(
    profile_id: &str,
    source: &mut SqliteConnection,
    write: &mut WriteSession,
    plan: &'static TablePlan,
) -> Result<(), UpgradeError> {
    let source_columns = table_columns(source, plan.name).await?;
    if source_columns.is_empty() {
        return Ok(());
    }
    let selected: Vec<&str> = plan
        .columns
        .iter()
        .copied()
        .filter(|column| source_columns.contains(*column))
        .collect();
    if selected.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "SELECT {} FROM {} ORDER BY rowid ASC",
        selected.join(", "),
        plan.name
    );
    let mut rows = sqlx::query(&sql).fetch(&mut *source);
    while let Some(row) = rows.try_next().await? {
        let mut values = BTreeMap::new();
        for (index, column) in row.columns().iter().enumerate() {
            values.insert(column.name().to_string(), decode_value(&row, index)?);
        }
        let mut imported = ImportedRow(values);
        apply_required_fallbacks(profile_id, plan.name, &mut imported)?;
        if plan.name == "collector_snapshots" {
            insert_synthetic_collector_run(write, &imported).await?;
        }
        insert_row(write, plan.name, imported).await?;
    }
    Ok(())
}

async fn table_columns(
    source: &mut SqliteConnection,
    table: &str,
) -> Result<BTreeSet<String>, UpgradeError> {
    let sql = format!("PRAGMA table_info({table})");
    Ok(sqlx::query(&sql)
        .fetch_all(source)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect())
}

fn decode_value(row: &sqlx::sqlite::SqliteRow, index: usize) -> Result<LegacyValue, UpgradeError> {
    let raw = row.try_get_raw(index)?;
    if raw.is_null() {
        return Ok(LegacyValue::Null);
    }
    match raw.type_info().name() {
        "INTEGER" | "BOOL" => Ok(LegacyValue::Integer(row.try_get(index)?)),
        "REAL" => Ok(LegacyValue::Real(row.try_get(index)?)),
        "BLOB" => Ok(LegacyValue::Blob(row.try_get(index)?)),
        _ => Ok(LegacyValue::Text(row.try_get(index)?)),
    }
}

fn apply_required_fallbacks(
    profile_id: &str,
    table_name: &str,
    row: &mut ImportedRow,
) -> Result<(), UpgradeError> {
    match table_name {
        "request_logs" => {
            let id = required_text(row, "id")?;
            let path = optional_text(row, "path").unwrap_or_else(|| "/v1/unknown".to_string());
            row.0.insert(
                "request_id".to_string(),
                LegacyValue::Text(format!("legacy:{profile_id}:{id}")),
            );
            ensure_text(row, "endpoint", path);
        }
        "pricing_rules" => {
            ensure_text(row, "normalization_status", "legacy_unverified".to_string())
        }
        "collector_runs" => {
            let id = required_text(row, "id")?;
            ensure_text(row, "run_key", format!("legacy:{profile_id}:{id}"));
            ensure_text(row, "request_hash", format!("legacy:{profile_id}:{id}"));
            ensure_integer(row, "endpoint_revision", 1);
        }
        "collector_snapshots" => {
            let id = required_text(row, "id")?;
            ensure_text(row, "run_id", id);
            ensure_integer(row, "endpoint_revision", 1);
        }
        "station_group_bindings" => {
            let id = required_text(row, "id")?;
            let station_id = required_text(row, "station_id")?;
            let group_name = required_text(row, "group_name")?;
            let binding_kind = if optional_text(row, "station_key_id").is_some() {
                "key_binding"
            } else {
                "station_group"
            };
            ensure_text(row, "binding_kind", binding_kind.to_string());
            ensure_text(
                row,
                "group_key_hash",
                stable_legacy_hash(profile_id, &[&station_id, &group_name, &id]),
            );
            ensure_text(
                row,
                "binding_status",
                if binding_kind == "key_binding" {
                    "bound"
                } else {
                    "available"
                }
                .to_string(),
            );
        }
        "group_rate_records" => {
            let id = required_text(row, "id")?;
            let station_id = required_text(row, "station_id")?;
            let group_name = required_text(row, "group_name")?;
            let binding_kind = if optional_text(row, "station_key_id").is_some() {
                "key_binding"
            } else {
                "station_group"
            };
            ensure_text(row, "binding_kind", binding_kind.to_string());
            ensure_text(
                row,
                "group_key_hash",
                stable_legacy_hash(profile_id, &[&station_id, &group_name, &id]),
            );
        }
        _ => {}
    }
    Ok(())
}

fn stable_legacy_hash(profile_id: &str, values: &[&str]) -> String {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(profile_id.as_bytes());
    for value in values {
        digest.update([0x1f]);
        digest.update(value.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn required_text(row: &ImportedRow, column: &str) -> Result<String, UpgradeError> {
    optional_text(row, column).ok_or(UpgradeError::ValidationFailed)
}

fn optional_text(row: &ImportedRow, column: &str) -> Option<String> {
    match row.0.get(column) {
        Some(LegacyValue::Text(value)) if !value.is_empty() => Some(value.clone()),
        Some(LegacyValue::Integer(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn ensure_text(row: &mut ImportedRow, column: &str, fallback: String) {
    if optional_text(row, column).is_none() {
        row.0
            .insert(column.to_string(), LegacyValue::Text(fallback));
    }
}

fn ensure_integer(row: &mut ImportedRow, column: &str, fallback: i64) {
    if !matches!(row.0.get(column), Some(LegacyValue::Integer(_))) {
        row.0
            .insert(column.to_string(), LegacyValue::Integer(fallback));
    }
}

async fn insert_row(
    write: &mut WriteSession,
    table_name: &str,
    ImportedRow(values): ImportedRow,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let columns: Vec<&str> = values.keys().map(String::as_str).collect();
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
        table_name,
        columns.join(", "),
        placeholders
    );
    let mut query = sqlx::query(&sql);
    for value in values.values() {
        query = match value {
            LegacyValue::Null => query.bind(Option::<String>::None),
            LegacyValue::Integer(value) => query.bind(*value),
            LegacyValue::Real(value) => query.bind(*value),
            LegacyValue::Text(value) => query.bind(value),
            LegacyValue::Blob(value) => query.bind(value),
        };
    }
    query.execute(write.connection()).await?;
    Ok(())
}

async fn insert_synthetic_collector_run(
    write: &mut WriteSession,
    snapshot: &ImportedRow,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let id = optional_text(snapshot, "run_id").ok_or_else(|| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "legacy collector snapshot is missing an id".to_string(),
        )
    })?;
    let station_id = optional_text(snapshot, "station_id").ok_or_else(|| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "legacy collector snapshot is missing station_id".to_string(),
        )
    })?;
    let source = optional_text(snapshot, "source").unwrap_or_else(|| "legacy".to_string());
    let status = optional_text(snapshot, "status").unwrap_or_else(|| "success".to_string());
    let started_at = optional_text(snapshot, "fetched_at")
        .or_else(|| optional_text(snapshot, "created_at"))
        .unwrap_or_else(|| "0".to_string());
    sqlx::query(
        r#"
            INSERT OR IGNORE INTO collector_runs (
                id, run_key, request_hash, station_id, endpoint_revision, adapter,
                task_type, status, started_at, finished_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, 1, ?5, 'legacy_snapshot', ?6, ?7, ?7, ?7)
            "#,
    )
    .bind(&id)
    .bind(format!("legacy-snapshot:{id}"))
    .bind(format!("legacy-snapshot:{id}"))
    .bind(station_id)
    .bind(source)
    .bind(status)
    .bind(started_at)
    .execute(write.connection())
    .await?;
    Ok(())
}

async fn load_secrets(
    profile_id: &str,
    source: &mut SqliteConnection,
    transformer: Option<&dyn LegacySecretTransformer>,
) -> Result<Vec<ImportedEncryptedSecret>, UpgradeError> {
    let mut materials = Vec::new();
    let mut encrypted_owners = BTreeSet::new();
    let secret_columns = table_columns(source, "secrets").await?;
    if ["scope", "owner_id", "kind", "ciphertext", "nonce", "aad"]
        .iter()
        .all(|column| secret_columns.contains(*column))
    {
        let rows = sqlx::query(
            r#"
            SELECT scope, owner_id, kind, ciphertext, nonce, aad
            FROM secrets
            ORDER BY scope, owner_id, kind, updated_at DESC, id DESC
            "#,
        )
        .fetch_all(&mut *source)
        .await?;
        for row in rows {
            let scope: String = row.try_get("scope")?;
            let owner_id: String = row.try_get("owner_id")?;
            let kind: String = row.try_get("kind")?;
            let ciphertext: String = row.try_get("ciphertext")?;
            let nonce: String = row.try_get("nonce")?;
            let aad: String = row.try_get("aad")?;
            if !encrypted_owners.insert((scope.clone(), owner_id.clone(), kind.clone())) {
                continue;
            }
            materials.push(LegacySecretMaterial::EncryptedV1 {
                scope,
                owner_id,
                kind,
                ciphertext: LegacySecretBytes::new(ciphertext.into_bytes()),
                nonce: LegacySecretBytes::new(nonce.into_bytes()),
                aad,
            });
        }
    }
    for (table, id_column, plaintext_column, scope, kind) in SECRET_SOURCES {
        let columns = table_columns(source, table).await?;
        if !columns.contains(*plaintext_column) {
            continue;
        }
        let sql = format!(
            "SELECT {id_column}, {plaintext_column} FROM {table} WHERE TRIM(COALESCE({plaintext_column}, '')) <> '' ORDER BY {id_column}"
        );
        for row in sqlx::query(&sql).fetch_all(&mut *source).await? {
            let owner_id: String = row.try_get(0)?;
            let plaintext: String = row.try_get(1)?;
            if encrypted_owners.contains(&(scope.to_string(), owner_id.clone(), kind.to_string())) {
                continue;
            }
            materials.push(LegacySecretMaterial::Plaintext {
                scope: (*scope).to_string(),
                owner_id,
                kind: (*kind).to_string(),
                value: LegacySecretBytes::new(plaintext.into_bytes()),
            });
        }
    }
    if materials.is_empty() {
        return Ok(Vec::new());
    }
    let transformer = transformer.ok_or(UpgradeError::SecretTransformerRequired)?;
    materials
        .into_iter()
        .map(|material| transformer.transform(profile_id, material))
        .collect()
}

async fn insert_secret(
    write: &mut WriteSession,
    secret: ImportedEncryptedSecret,
) -> Result<(), crate::persistence::error::PersistenceError> {
    sqlx::query(
        r#"
        INSERT INTO secrets (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(&secret.id)
    .bind(&secret.scope)
    .bind(&secret.owner_id)
    .bind(&secret.kind)
    .bind(&secret.masked_value)
    .bind(&secret.ciphertext)
    .bind(&secret.nonce)
    .bind(&secret.created_at)
    .bind(&secret.updated_at)
    .execute(write.connection())
    .await?;
    let (table, id_column, reference_column) = match (secret.scope.as_str(), secret.kind.as_str()) {
        ("station", "api_key") => ("stations", "id", "api_key_secret_id"),
        ("station_key", "api_key") => ("station_keys", "id", "api_key_secret_id"),
        ("station_credentials", "login_password") => (
            "station_credentials",
            "station_id",
            "login_password_secret_id",
        ),
        ("station_credentials", "access_token") => (
            "station_credentials",
            "station_id",
            "access_token_secret_id",
        ),
        ("station_credentials", "refresh_token") => (
            "station_credentials",
            "station_id",
            "refresh_token_secret_id",
        ),
        ("station_credentials", "cookie") => {
            ("station_credentials", "station_id", "cookie_secret_id")
        }
        _ => {
            return Err(
                crate::persistence::error::PersistenceError::InvariantViolation(
                    "unsupported imported secret reference".to_string(),
                ),
            )
        }
    };
    let sql = format!("UPDATE {table} SET {reference_column} = ?1 WHERE {id_column} = ?2");
    let updated = sqlx::query(&sql)
        .bind(secret.id)
        .bind(secret.owner_id)
        .execute(write.connection())
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(
            crate::persistence::error::PersistenceError::InvariantViolation(
                "imported secret owner is missing or ambiguous".to_string(),
            ),
        );
    }
    Ok(())
}

struct TablePlan {
    name: &'static str,
    columns: &'static [&'static str],
}

const TABLE_PLANS: &[TablePlan] = &[
    TablePlan {
        name: "settings",
        columns: &["key", "value", "updated_at"],
    },
    TablePlan {
        name: "stations",
        columns: &[
            "id",
            "name",
            "station_type",
            "website_url",
            "api_base_url",
            "endpoint_revision",
            "upstream_api_format",
            "collector_proxy_mode",
            "collector_proxy_url",
            "enabled",
            "priority",
            "credit_per_cny",
            "balance_raw",
            "balance_cny",
            "low_balance_threshold_cny",
            "collection_interval_minutes",
            "status",
            "latency_ms",
            "last_checked_at",
            "last_pricing_fetched_at",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "station_credentials",
        columns: &[
            "station_id",
            "login_username",
            "remember_password",
            "login_status",
            "login_error",
            "last_login_at",
            "session_status",
            "session_expires_at",
            "newapi_user_id",
            "token_expires_at",
            "token_refreshed_at",
            "session_source",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "station_keys",
        columns: &[
            "id",
            "station_id",
            "name",
            "enabled",
            "priority",
            "routing_order",
            "max_concurrency",
            "load_factor",
            "schedulable",
            "group_name",
            "tier_label",
            "group_binding_id",
            "group_id_hash",
            "rate_multiplier",
            "manual_rate_multiplier",
            "manual_rate_updated_at",
            "rate_source",
            "rate_collected_at",
            "balance_scope",
            "status",
            "last_checked_at",
            "last_used_at",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "remote_station_keys",
        columns: &[
            "id",
            "station_id",
            "remote_key_id_hash",
            "remote_key_name",
            "api_key_masked",
            "api_key_fingerprint",
            "group_id_hash",
            "group_name",
            "tier_label",
            "rate_multiplier",
            "rate_source",
            "created_at",
            "last_used_at",
            "raw_source",
            "match_status",
            "matched_station_key_id",
            "match_confidence",
            "collected_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "station_key_capabilities",
        columns: &[
            "station_key_id",
            "supports_chat_completions",
            "supports_responses",
            "supports_embeddings",
            "supports_stream",
            "supports_tools",
            "supports_vision",
            "supports_reasoning",
            "model_allowlist_json",
            "model_blocklist_json",
            "preferred_models_json",
            "only_use_as_backup",
            "routing_tags_json",
            "updated_at",
        ],
    },
    TablePlan {
        name: "model_aliases",
        columns: &[
            "id",
            "client_model",
            "upstream_model",
            "enabled",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "collector_runs",
        columns: &[
            "id",
            "run_key",
            "request_hash",
            "station_id",
            "endpoint_revision",
            "parent_run_id",
            "adapter",
            "task_type",
            "status",
            "started_at",
            "finished_at",
            "duration_ms",
            "endpoint_count",
            "success_count",
            "failure_count",
            "manual_action_required",
            "error_code",
            "error_message",
            "snapshot_id",
            "created_at",
        ],
    },
    TablePlan {
        name: "collector_snapshots",
        columns: &[
            "id",
            "run_id",
            "station_id",
            "endpoint_revision",
            "source",
            "status",
            "fetched_at",
            "summary_json",
            "normalized_json",
            "raw_json_redacted",
            "error_message",
            "created_at",
        ],
    },
    TablePlan {
        name: "station_group_bindings",
        columns: &[
            "id",
            "station_id",
            "station_key_id",
            "binding_kind",
            "parent_group_binding_id",
            "group_key_hash",
            "group_id_hash",
            "group_name",
            "binding_status",
            "default_rate_multiplier",
            "user_rate_multiplier",
            "effective_rate_multiplier",
            "inferred_group_category",
            "group_category_override",
            "rate_source",
            "confidence",
            "last_seen_at",
            "last_checked_at",
            "last_rate_changed_at",
            "last_seen_run_id",
            "raw_json_redacted",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "group_rate_records",
        columns: &[
            "id",
            "station_id",
            "station_key_id",
            "group_binding_id",
            "binding_kind",
            "group_key_hash",
            "group_name",
            "default_rate_multiplier",
            "user_rate_multiplier",
            "effective_rate_multiplier",
            "inferred_group_category",
            "source",
            "confidence",
            "raw_json_redacted",
            "checked_at",
            "created_at",
        ],
    },
    TablePlan {
        name: "pricing_rules",
        columns: &[
            "id",
            "station_id",
            "station_key_id",
            "group_binding_id",
            "group_name",
            "tier_label",
            "model",
            "input_price",
            "output_price",
            "fixed_price",
            "rate_multiplier",
            "currency",
            "unit",
            "price_type",
            "base_price_source",
            "normalization_status",
            "source",
            "confidence",
            "enabled",
            "note",
            "collected_at",
            "valid_from",
            "valid_until",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "model_base_prices",
        columns: &[
            "id",
            "provider",
            "model",
            "input_price",
            "output_price",
            "currency",
            "unit",
            "source_url",
            "source_label",
            "source_checked_at",
            "enabled",
            "built_in",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "balance_snapshots",
        columns: &[
            "id",
            "station_id",
            "station_key_id",
            "scope",
            "value",
            "currency",
            "credit_unit",
            "used_value",
            "total_value",
            "today_request_count",
            "total_request_count",
            "today_consumption",
            "total_consumption",
            "today_base_consumption",
            "total_base_consumption",
            "today_token_count",
            "total_token_count",
            "today_input_token_count",
            "today_output_token_count",
            "total_input_token_count",
            "total_output_token_count",
            "account_concurrency_limit",
            "low_balance_threshold",
            "status",
            "source",
            "confidence",
            "collected_at",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "channel_monitor_request_templates",
        columns: &[
            "id",
            "name",
            "endpoint_kind",
            "method",
            "path",
            "request_body_json",
            "enabled",
            "built_in",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "channel_monitors",
        columns: &[
            "id",
            "name",
            "target_type",
            "station_id",
            "station_key_id",
            "template_id",
            "enabled",
            "interval_seconds",
            "jitter_seconds",
            "timeout_seconds",
            "max_concurrency",
            "consecutive_failure_threshold",
            "fallback_models_json",
            "last_run_at",
            "next_run_at",
            "last_status",
            "last_error_message",
            "note",
            "created_at",
            "updated_at",
        ],
    },
    TablePlan {
        name: "channel_monitor_runs",
        columns: &[
            "id",
            "monitor_id",
            "template_id",
            "station_id",
            "station_key_id",
            "status",
            "started_at",
            "finished_at",
            "duration_ms",
            "http_status",
            "latency_ms",
            "response_model",
            "fallback_model",
            "error_message",
            "created_at",
        ],
    },
    TablePlan {
        name: "request_logs",
        columns: &[
            "id",
            "request_id",
            "started_at",
            "finished_at",
            "duration_ms",
            "method",
            "path",
            "endpoint",
            "model",
            "stream",
            "status",
            "lifecycle_status",
            "station_key_id",
            "station_id",
            "upstream_base_url",
            "fallback_count",
            "error_message",
            "route_policy",
            "route_reason",
            "rejected_candidates_json",
            "body_bytes",
            "attempt_count",
            "route_wait_ms",
            "upstream_headers_ms",
            "failure_source",
            "attempts_json",
            "completion_source",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "cache_creation_tokens",
            "cache_read_tokens",
            "reasoning_effort",
            "first_token_ms",
            "terminal_kind",
            "terminal_code",
            "terminal_detail",
            "protocol_completed",
            "delivery_terminal",
            "selected_attempt_ordinal",
            "terminal_at_ms",
            "created_at",
        ],
    },
    TablePlan {
        name: "station_key_health",
        columns: &[
            "station_key_id",
            "endpoint_revision",
            "last_success_at",
            "last_failure_at",
            "consecutive_failures",
            "success_count",
            "failure_count",
            "total_duration_ms",
            "avg_latency_ms",
            "last_error_summary",
            "cooldown_until",
            "updated_at",
        ],
    },
    TablePlan {
        name: "station_endpoint_health",
        columns: &[
            "station_id",
            "endpoint_revision",
            "status",
            "latency_ms",
            "checked_at",
            "error_summary",
            "updated_at",
        ],
    },
    TablePlan {
        name: "change_events",
        columns: &[
            "id",
            "severity",
            "event_type",
            "status",
            "title",
            "message",
            "object_type",
            "object_id",
            "station_id",
            "station_key_id",
            "pricing_rule_id",
            "request_log_id",
            "old_value_json",
            "new_value_json",
            "impact_json",
            "dedupe_key",
            "source",
            "detected_at",
            "resolved_at",
            "created_at",
            "updated_at",
        ],
    },
];

const SECRET_SOURCES: &[(&str, &str, &str, &str, &str)] = &[
    ("stations", "id", "api_key", "station", "api_key"),
    ("station_keys", "id", "api_key", "station_key", "api_key"),
    (
        "station_credentials",
        "station_id",
        "login_password",
        "station_credentials",
        "login_password",
    ),
];
