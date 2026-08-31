use serde_json::Value;
use sqlx::Row;

use crate::{
    application::routing_generation::{
        canonical_json_bytes, canonical_json_sha256, policy_generation_id,
    },
    models::routing_policy::{
        RoutingPolicyConfigV2, RoutingPolicyConfigV3, RoutingPolicyV3UpgradeAudit,
        ROUTING_POLICY_CONFIG_VERSION_V3,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::{
            routing_generation_store::RoutingGenerationStore,
            routing_policy_store::{RoutingPolicyStore, StoredRoutingPolicy},
        },
    },
};

const POLICY_GENERATION_ALGORITHM_VERSION: &str = "1";
const TARGET_POLICY_VERSION: &str = "routing-policy-v3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredRoutingPolicyPublication {
    pub(crate) revision: u64,
    pub(crate) policy_generation_id: String,
    pub(crate) policy_status: String,
    pub(crate) policy_failure_code: Option<String>,
    pub(crate) policy_updated_at_ms: i64,
    pub(crate) runtime_status: Option<String>,
    pub(crate) runtime_failure_code: Option<String>,
    pub(crate) runtime_updated_at_ms: Option<i64>,
}

#[derive(Debug)]
struct SourcePolicy {
    scope: &'static str,
    revision: u64,
    policy_version: String,
    system_version: String,
    value: Value,
}

#[derive(Debug)]
struct PreparedPolicy {
    source: SourcePolicy,
    policy_generation_id: String,
    canonical_policy_hash: String,
    canonical_config_json: String,
    source_fields_json: String,
    defaulted_fields_json: String,
    discarded_fields_json: String,
    semantic_changes_json: String,
    quality_rebuild_required: bool,
}

/// Materializes the v3 policy staging set after schema 60 is installed.
/// Every source row is decoded before any row is written, and staged/audit
/// rows commit together so malformed history cannot leave a partial upgrade.
pub(crate) async fn stage_all(
    runtime: &PersistenceHandle,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    if now_ms < 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    let mut write = runtime.begin_write().await?;
    let sources = load_sources(write.connection()).await?;
    let prepared = sources
        .into_iter()
        .map(prepare)
        .collect::<Result<Vec<_>, _>>()?;

    for policy in &prepared {
        insert_staged(write.connection(), policy, now_ms).await?;
        insert_audit(write.connection(), policy, now_ms).await?;
        validate_persisted(write.connection(), policy).await?;
    }
    write.commit().await
}

pub(crate) async fn load_effective_active(
    runtime: &PersistenceHandle,
) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    load_effective_active_in(read.connection()).await
}

/// Loads the durable publication facts for one immutable policy revision.
/// The runtime row is the newest build for that policy generation so a stale
/// policy-level `ready` flag cannot hide a later failed rebuild.
pub(crate) async fn load_publication_by_revision(
    connection: &mut sqlx::SqliteConnection,
    revision: u64,
) -> Result<Option<StoredRoutingPolicyPublication>, PersistenceError> {
    if revision == 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    let row = sqlx::query(
        "SELECT p.target_policy_revision, p.policy_generation_id,
                p.status AS policy_status,
                p.failure_code AS policy_failure_code,
                p.updated_at_ms AS policy_updated_at_ms,
                r.status AS runtime_status,
                r.failure_code AS runtime_failure_code,
                r.updated_at_ms AS runtime_updated_at_ms
         FROM routing_policy_v3_staged p
         LEFT JOIN routing_runtime_generation r
           ON r.runtime_generation_id = (
               SELECT candidate.runtime_generation_id
               FROM routing_runtime_generation candidate
               WHERE candidate.policy_generation_id = p.policy_generation_id
               ORDER BY candidate.created_at_ms DESC,
                        candidate.runtime_generation_id ASC
               LIMIT 1
           )
         WHERE p.scope = 'active' AND p.target_policy_revision = ?1",
    )
    .bind(to_i64(revision)?)
    .fetch_optional(&mut *connection)
    .await?;

    row.map(|row| {
        Ok(StoredRoutingPolicyPublication {
            revision: u64::try_from(row.get::<i64, _>("target_policy_revision"))
                .map_err(|_| PersistenceError::ConstraintViolation)?,
            policy_generation_id: row.get("policy_generation_id"),
            policy_status: row.get("policy_status"),
            policy_failure_code: row.get("policy_failure_code"),
            policy_updated_at_ms: row.get("policy_updated_at_ms"),
            runtime_status: row.get("runtime_status"),
            runtime_failure_code: row.get("runtime_failure_code"),
            runtime_updated_at_ms: row.get("runtime_updated_at_ms"),
        })
    })
    .transpose()
}

/// Stages a user policy against the single active generation. This operation
/// never mutates the active pointer or the legacy compatibility row.
pub(crate) async fn stage_user_policy(
    runtime: &PersistenceHandle,
    expected_active_revision: u64,
    policy: &RoutingPolicyConfigV3,
    source_kind: &str,
    now_ms: i64,
) -> Result<StoredRoutingPolicy, PersistenceError> {
    policy
        .validate()
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if expected_active_revision == 0
        || now_ms < 0
        || !matches!(source_kind, "user" | "file_sync" | "restore" | "system")
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    let value = serde_json::to_value(policy)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let canonical_policy_hash =
        canonical_json_sha256(&value).map_err(|_| PersistenceError::ConstraintViolation)?;
    let canonical_config_json = String::from_utf8(
        canonical_json_bytes(&value).map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .map_err(|_| PersistenceError::ConstraintViolation)?;
    let mut write = runtime.begin_write().await?;
    let active = load_effective_active_in(write.connection())
        .await?
        .ok_or(PersistenceError::NotFound)?;
    let active_value = routing_policy_v3_value(&active)?;
    let generation_id = policy_generation_id(
        "active",
        active.revision,
        TARGET_POLICY_VERSION,
        &canonical_policy_hash,
        POLICY_GENERATION_ALGORITHM_VERSION,
    )
    .map_err(|_| PersistenceError::ConstraintViolation)?;
    // The public snapshot returned after a save carries the staged target
    // revision. Accept that revision for a subsequent edit, but keep the
    // source identity anchored to the current active policy. This allows a
    // user to make consecutive edits while preserving CAS protection against
    // an unrelated active-policy change.
    let base_value = if expected_active_revision == active.revision {
        active_value.clone()
    } else {
        let staged = sqlx::query(
            "SELECT config_json, status FROM routing_policy_v3_staged
             WHERE scope = 'active' AND target_policy_revision = ?1",
        )
        .bind(to_i64(expected_active_revision)?)
        .fetch_optional(write.connection())
        .await?;
        let Some(staged) = staged else {
            return Err(PersistenceError::RevisionConflict("routing_policy".into()));
        };
        if staged.get::<String, _>("status") != "staged" {
            return Err(PersistenceError::RevisionConflict("routing_policy".into()));
        }
        serde_json::from_str(&staged.get::<String, _>("config_json"))
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?
    };
    if base_value == value {
        if expected_active_revision == active.revision {
            return Ok(active);
        }
        return load_staged_by_revision(write.connection(), expected_active_revision)
            .await?
            .ok_or(PersistenceError::RevisionConflict("routing_policy".into()));
    }
    if let Some(existing) = load_staged_by_generation(write.connection(), &generation_id).await? {
        return Ok(existing);
    }
    let max_revision = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(target_policy_revision) FROM routing_policy_v3_staged WHERE scope = 'active'",
    )
    .fetch_one(write.connection())
    .await?
    .unwrap_or(0);
    let target_revision = u64::try_from(max_revision)
        .unwrap_or(0)
        .max(expected_active_revision)
        .checked_add(1)
        .ok_or(PersistenceError::ConstraintViolation)?;
    let quality_rebuild_required = quality_inputs_changed(&active_value, &value)?;
    sqlx::query(
        "INSERT INTO routing_policy_v3_staged (
             scope, source_config_revision, target_policy_revision,
             config_revision, policy_generation_id, canonical_policy_hash,
             policy_algorithm_version, source_policy_version, system_version,
             target_policy_version, staged_policy_version, config_json,
             status, created_at_ms, updated_at_ms
         ) VALUES ('active', ?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7,
                   ?8, ?8, ?9, 'staged', ?10, ?10)",
    )
    .bind(to_i64(active.revision)?)
    .bind(to_i64(target_revision)?)
    .bind(&generation_id)
    .bind(&canonical_policy_hash)
    .bind(POLICY_GENERATION_ALGORITHM_VERSION)
    .bind(&active.policy_version)
    .bind(&active.system_version)
    .bind(TARGET_POLICY_VERSION)
    .bind(&canonical_config_json)
    .bind(now_ms)
    .execute(write.connection())
    .await?;
    let source_fields_json = source_field_names(&value)?;
    let semantic_changes_json = serde_json::to_string(&[source_kind])
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    sqlx::query(
        "INSERT INTO routing_policy_v3_migration_audit (
             scope, source_config_revision, target_policy_revision,
             target_policy_version, policy_generation_id, migration_status,
             source_fields_json, defaulted_fields_json, discarded_fields_json,
             semantic_changes_json, quality_rebuild_required, created_at_ms
         ) VALUES ('active', ?1, ?2, ?3, ?4, 'staged', ?5,
                   '[]', '[]', ?6, ?7, ?8)",
    )
    .bind(to_i64(active.revision)?)
    .bind(to_i64(target_revision)?)
    .bind(TARGET_POLICY_VERSION)
    .bind(&generation_id)
    .bind(source_fields_json)
    .bind(semantic_changes_json)
    .bind(quality_rebuild_required)
    .bind(now_ms)
    .execute(write.connection())
    .await?;
    write.commit().await?;
    Ok(StoredRoutingPolicy {
        config: value,
        revision: target_revision,
        policy_version: TARGET_POLICY_VERSION.to_string(),
        system_version: active.system_version,
        status: "staged".to_string(),
        updated_at_ms: now_ms,
    })
}

pub(crate) async fn load_effective_active_in(
    connection: &mut sqlx::SqliteConnection,
) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
    let registry = RoutingGenerationStore
        .load_registry_snapshot(connection)
        .await?;
    match registry.marker.mode {
        crate::models::routing_generation::RoutingCutoverMode::PreCutover => {
            RoutingPolicyStore.load(connection).await
        }
        crate::models::routing_generation::RoutingCutoverMode::V3Active => {
            let active = registry.active.ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "routing_generation_registry_corrupt: active policy is missing".into(),
                )
            })?;
            let row = sqlx::query(
                "SELECT config_json, target_policy_revision,
                        target_policy_version, system_version, status, updated_at_ms
                 FROM routing_policy_v3_staged WHERE policy_generation_id = ?1",
            )
            .bind(&active.policy_generation_id)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "routing_generation_registry_corrupt: staged policy is missing".into(),
                )
            })?;
            if row.get::<String, _>("status") != "active" {
                return Err(PersistenceError::InvariantViolation(
                    "routing_generation_registry_corrupt: staged policy is not active".into(),
                ));
            }
            let config = serde_json::from_str(&row.get::<String, _>("config_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
            Ok(Some(StoredRoutingPolicy {
                config,
                revision: u64::try_from(row.get::<i64, _>("target_policy_revision"))
                    .map_err(|_| PersistenceError::ConstraintViolation)?,
                policy_version: row.get("target_policy_version"),
                system_version: row.get("system_version"),
                status: "active".to_string(),
                updated_at_ms: row.get("updated_at_ms"),
            }))
        }
    }
}

async fn load_staged_by_generation(
    connection: &mut sqlx::SqliteConnection,
    generation_id: &str,
) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
    sqlx::query(
        "SELECT config_json, target_policy_revision, target_policy_version,
                system_version, status, updated_at_ms
         FROM routing_policy_v3_staged WHERE policy_generation_id = ?1",
    )
    .bind(generation_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(|row| {
        Ok(StoredRoutingPolicy {
            config: serde_json::from_str(&row.get::<String, _>("config_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
            revision: u64::try_from(row.get::<i64, _>("target_policy_revision"))
                .map_err(|_| PersistenceError::ConstraintViolation)?,
            policy_version: row.get("target_policy_version"),
            system_version: row.get("system_version"),
            status: row.get("status"),
            updated_at_ms: row.get("updated_at_ms"),
        })
    })
    .transpose()
}

async fn load_staged_by_revision(
    connection: &mut sqlx::SqliteConnection,
    revision: u64,
) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
    sqlx::query(
        "SELECT config_json, target_policy_revision, target_policy_version,
                system_version, status, updated_at_ms
         FROM routing_policy_v3_staged
         WHERE scope = 'active' AND target_policy_revision = ?1",
    )
    .bind(to_i64(revision)?)
    .fetch_optional(&mut *connection)
    .await?
    .map(|row| {
        Ok(StoredRoutingPolicy {
            config: serde_json::from_str(&row.get::<String, _>("config_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
            revision: u64::try_from(row.get::<i64, _>("target_policy_revision"))
                .map_err(|_| PersistenceError::ConstraintViolation)?,
            policy_version: row.get("target_policy_version"),
            system_version: row.get("system_version"),
            status: row.get("status"),
            updated_at_ms: row.get("updated_at_ms"),
        })
    })
    .transpose()
}

fn routing_policy_v3_value(active: &StoredRoutingPolicy) -> Result<Value, PersistenceError> {
    let policy = if active.config.get("version").and_then(Value::as_u64)
        == Some(u64::from(ROUTING_POLICY_CONFIG_VERSION_V3))
    {
        RoutingPolicyConfigV3::from_stored_value(&active.config)
            .map_err(|_| PersistenceError::ConstraintViolation)?
    } else {
        let v2 = RoutingPolicyConfigV2::from_stored_value(&active.config)
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        RoutingPolicyConfigV3::from_v2(&v2)
            .map_err(|_| PersistenceError::ConstraintViolation)?
            .policy
    };
    serde_json::to_value(policy)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))
}

fn quality_inputs_changed(left: &Value, right: &Value) -> Result<bool, PersistenceError> {
    let left = RoutingPolicyConfigV3::from_stored_value(left)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let right = RoutingPolicyConfigV3::from_stored_value(right)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    Ok(
        left.reliability_source_weights != right.reliability_source_weights
            || left.reliability_sampling != right.reliability_sampling,
    )
}

async fn load_sources(
    connection: &mut sqlx::SqliteConnection,
) -> Result<Vec<SourcePolicy>, PersistenceError> {
    let active = sqlx::query(
        "SELECT config_revision, policy_version, system_version, config_json
         FROM routing_policy WHERE singleton_key = 1",
    )
    .fetch_optional(&mut *connection)
    .await?;
    let mut sources = Vec::new();
    if let Some(row) = active {
        sources.push(source_from_row("active", row)?);
    }
    let history = sqlx::query(
        "SELECT config_revision, policy_version, system_version, config_json
         FROM routing_policy_history ORDER BY config_revision",
    )
    .fetch_all(&mut *connection)
    .await?;
    for row in history {
        sources.push(source_from_row("history", row)?);
    }
    Ok(sources)
}

fn source_from_row(
    scope: &'static str,
    row: sqlx::sqlite::SqliteRow,
) -> Result<SourcePolicy, PersistenceError> {
    let revision = u64::try_from(row.get::<i64, _>("config_revision"))
        .map_err(|_| migration_invalid(scope, 0, "config_revision", "invalid_revision"))?;
    if revision == 0 {
        return Err(migration_invalid(
            scope,
            revision,
            "config_revision",
            "invalid_revision",
        ));
    }
    let raw = row.get::<String, _>("config_json");
    let value = serde_json::from_str(&raw)
        .map_err(|_| migration_invalid(scope, revision, "policy", "invalid_json"))?;
    Ok(SourcePolicy {
        scope,
        revision,
        policy_version: row.get("policy_version"),
        system_version: row.get("system_version"),
        value,
    })
}

fn prepare(source: SourcePolicy) -> Result<PreparedPolicy, PersistenceError> {
    let source_fields_json = source_field_names(&source.value)?;
    let (policy, audit) = if source.value.get("version").and_then(Value::as_u64)
        == Some(u64::from(ROUTING_POLICY_CONFIG_VERSION_V3))
    {
        let policy = RoutingPolicyConfigV3::from_stored_value(&source.value).map_err(|error| {
            migration_invalid(source.scope, source.revision, error.field, error.code)
        })?;
        (policy, canonical_v3_audit())
    } else {
        let v2 = RoutingPolicyConfigV2::from_stored_value(&source.value).map_err(|error| {
            migration_invalid(source.scope, source.revision, error.field, error.code)
        })?;
        let upgraded = RoutingPolicyConfigV3::from_v2(&v2).map_err(|error| {
            migration_invalid(source.scope, source.revision, error.field, error.code)
        })?;
        (upgraded.policy, upgraded.audit)
    };
    let value = serde_json::to_value(policy)
        .map_err(|_| migration_invalid(source.scope, source.revision, "policy", "serialize"))?;
    let canonical_policy_hash = canonical_json_sha256(&value)
        .map_err(|_| migration_invalid(source.scope, source.revision, "policy", "hash"))?;
    let canonical_config_json =
        String::from_utf8(canonical_json_bytes(&value).map_err(|_| {
            migration_invalid(source.scope, source.revision, "policy", "canonical")
        })?)
        .map_err(|_| migration_invalid(source.scope, source.revision, "policy", "canonical"))?;
    let generation_id = policy_generation_id(
        source.scope,
        source.revision,
        TARGET_POLICY_VERSION,
        &canonical_policy_hash,
        POLICY_GENERATION_ALGORITHM_VERSION,
    )
    .map_err(|_| migration_invalid(source.scope, source.revision, "policy", "generation_id"))?;
    Ok(PreparedPolicy {
        source,
        policy_generation_id: generation_id,
        canonical_policy_hash,
        canonical_config_json,
        source_fields_json,
        defaulted_fields_json: json_names(&audit.defaulted_fields)?,
        discarded_fields_json: json_names(&audit.discarded_fields)?,
        semantic_changes_json: json_names(&audit.semantic_changes)?,
        quality_rebuild_required: audit.quality_rebuild_required,
    })
}

async fn insert_staged(
    connection: &mut sqlx::SqliteConnection,
    policy: &PreparedPolicy,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO routing_policy_v3_staged (
             scope, source_config_revision, target_policy_revision,
             config_revision, policy_generation_id, canonical_policy_hash,
             policy_algorithm_version, source_policy_version,
             system_version, target_policy_version, staged_policy_version,
             config_json, status, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9,
                   'staged', ?10, ?10)
         ON CONFLICT(policy_generation_id) DO NOTHING",
    )
    .bind(policy.source.scope)
    .bind(to_i64(policy.source.revision)?)
    .bind(&policy.policy_generation_id)
    .bind(&policy.canonical_policy_hash)
    .bind(POLICY_GENERATION_ALGORITHM_VERSION)
    .bind(&policy.source.policy_version)
    .bind(&policy.source.system_version)
    .bind(TARGET_POLICY_VERSION)
    .bind(&policy.canonical_config_json)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn insert_audit(
    connection: &mut sqlx::SqliteConnection,
    policy: &PreparedPolicy,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO routing_policy_v3_migration_audit (
             scope, source_config_revision, target_policy_revision,
             target_policy_version, policy_generation_id, migration_status,
             source_fields_json, defaulted_fields_json, discarded_fields_json,
             semantic_changes_json, quality_rebuild_required, created_at_ms
         ) VALUES (?1, ?2, ?2, ?3, ?4, 'staged', ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(policy_generation_id) DO NOTHING",
    )
    .bind(policy.source.scope)
    .bind(to_i64(policy.source.revision)?)
    .bind(TARGET_POLICY_VERSION)
    .bind(&policy.policy_generation_id)
    .bind(&policy.source_fields_json)
    .bind(&policy.defaulted_fields_json)
    .bind(&policy.discarded_fields_json)
    .bind(&policy.semantic_changes_json)
    .bind(policy.quality_rebuild_required)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn validate_persisted(
    connection: &mut sqlx::SqliteConnection,
    policy: &PreparedPolicy,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT source_config_revision, target_policy_revision,
                canonical_policy_hash, config_json
         FROM routing_policy_v3_staged WHERE policy_generation_id = ?1",
    )
    .bind(&policy.policy_generation_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| {
        migration_invalid(
            policy.source.scope,
            policy.source.revision,
            "policy",
            "missing",
        )
    })?;
    let revision = to_i64(policy.source.revision)?;
    if row.get::<i64, _>("source_config_revision") != revision
        || row.get::<i64, _>("target_policy_revision") != revision
        || row.get::<String, _>("canonical_policy_hash") != policy.canonical_policy_hash
        || row.get::<String, _>("config_json") != policy.canonical_config_json
    {
        return Err(migration_invalid(
            policy.source.scope,
            policy.source.revision,
            "policy",
            "identity_collision",
        ));
    }
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_policy_v3_migration_audit
         WHERE policy_generation_id = ?1",
    )
    .bind(&policy.policy_generation_id)
    .fetch_one(&mut *connection)
    .await?;
    if audit_count != 1 {
        return Err(migration_invalid(
            policy.source.scope,
            policy.source.revision,
            "audit",
            "postcondition",
        ));
    }
    Ok(())
}

fn canonical_v3_audit() -> RoutingPolicyV3UpgradeAudit {
    RoutingPolicyV3UpgradeAudit {
        from_version: ROUTING_POLICY_CONFIG_VERSION_V3,
        to_version: ROUTING_POLICY_CONFIG_VERSION_V3,
        discarded_fields: Vec::new(),
        defaulted_fields: Vec::new(),
        semantic_changes: Vec::new(),
        quality_rebuild_required: true,
    }
}

fn source_field_names(value: &Value) -> Result<String, PersistenceError> {
    let Some(object) = value.as_object() else {
        return Err(PersistenceError::ConstraintViolation);
    };
    let mut fields = object.keys().map(String::as_str).collect::<Vec<_>>();
    fields.sort_unstable();
    serde_json::to_string(&fields)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))
}

fn json_names(values: &[&str]) -> Result<String, PersistenceError> {
    serde_json::to_string(values)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
}

fn migration_invalid(scope: &str, revision: u64, field: &str, code: &str) -> PersistenceError {
    PersistenceError::InvariantViolation(format!(
        "routing_policy_migration_invalid:{scope}:{revision}:{field}:{code}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn stages_every_source_once_with_canonical_generation_identity() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("routing.sqlite3"))
            .await
            .expect("runtime");

        stage_all(&runtime.handle(), 10).await.expect("first stage");
        stage_all(&runtime.handle(), 20)
            .await
            .expect("idempotent replay");

        let mut read = runtime.handle().begin_read().await.expect("read");
        let row = sqlx::query(
            "SELECT source_config_revision, target_policy_revision,
                    policy_generation_id, canonical_policy_hash,
                    policy_algorithm_version, config_json
             FROM routing_policy_v3_staged WHERE scope = 'active'",
        )
        .fetch_one(read.connection())
        .await
        .expect("active staged policy");
        assert_eq!(
            row.get::<i64, _>("source_config_revision"),
            row.get::<i64, _>("target_policy_revision")
        );
        assert!(row
            .get::<String, _>("policy_generation_id")
            .starts_with("pg1_"));
        assert_eq!(row.get::<String, _>("canonical_policy_hash").len(), 64);
        assert_eq!(row.get::<String, _>("policy_algorithm_version"), "1");
        let value = serde_json::from_str::<Value>(&row.get::<String, _>("config_json"))
            .expect("canonical v3 JSON");
        assert_eq!(value["version"], 3);
        assert!(value.get("maxCandidates").is_none());
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM routing_policy_v3_staged),
                (SELECT COUNT(*) FROM routing_policy_v3_migration_audit)",
        )
        .fetch_one(read.connection())
        .await
        .expect("stage counts");
        assert_eq!(counts.0, counts.1);
        drop(read);
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn malformed_history_rolls_back_the_complete_stage_set() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("routing.sqlite3"))
            .await
            .expect("runtime");
        let mut write = runtime.handle().begin_write().await.expect("write");
        sqlx::query(
            "INSERT INTO routing_policy_history (
                 config_revision, config_json, policy_version,
                 system_version, status, created_at_ms
             ) VALUES (999, '{}', 'routing-policy-v2',
                       'routing-system-v1', 'active', 1)",
        )
        .execute(write.connection())
        .await
        .expect("malformed history fixture");
        write.commit().await.expect("commit fixture");

        let error = stage_all(&runtime.handle(), 10)
            .await
            .expect_err("invalid history must fail the full data stage");
        assert!(matches!(
            error,
            PersistenceError::InvariantViolation(ref detail)
                if detail.contains("routing_policy_migration_invalid:history:999")
        ));
        let mut read = runtime.handle().begin_read().await.expect("read");
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM routing_policy_v3_staged),
                (SELECT COUNT(*) FROM routing_policy_v3_migration_audit)",
        )
        .fetch_one(read.connection())
        .await
        .expect("stage counts");
        assert_eq!(counts, (0, 0));
        drop(read);
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn publication_query_reads_staged_state_and_latest_runtime_failure() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("routing.sqlite3"))
            .await
            .expect("runtime");
        stage_all(&runtime.handle(), 10)
            .await
            .expect("stage policy");

        let mut read = runtime.handle().begin_read().await.expect("read");
        let identity: (i64, String) = sqlx::query_as(
            "SELECT target_policy_revision, policy_generation_id
             FROM routing_policy_v3_staged WHERE scope = 'active'",
        )
        .fetch_one(read.connection())
        .await
        .expect("staged identity");
        let revision = u64::try_from(identity.0).expect("positive revision");
        let staged = load_publication_by_revision(read.connection(), revision)
            .await
            .expect("publication query")
            .expect("staged publication");
        assert_eq!(staged.policy_status, "staged");
        assert_eq!(staged.runtime_status, None);
        drop(read);

        let mut write = runtime.handle().begin_write().await.expect("write");
        for suffix in ["old", "new"] {
            sqlx::query(
                "INSERT INTO routing_quality_generation_v3 (
                     quality_generation_id, scope, quality_policy_revision,
                     quality_algorithm_version, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, 'active', ?2, '1', 'building', 10, 10)",
            )
            .bind(format!("quality-{suffix}"))
            .bind(identity.0)
            .execute(write.connection())
            .await
            .expect("quality generation fixture");
            sqlx::query(
                "INSERT INTO routing_circuit_generation_v3 (
                     circuit_generation_id, scope, circuit_policy_revision,
                     circuit_algorithm_version, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, 'active', ?2, '1', 'building', 10, 10)",
            )
            .bind(format!("circuit-{suffix}"))
            .bind(identity.0)
            .execute(write.connection())
            .await
            .expect("circuit generation fixture");
        }
        let hash = "0".repeat(64);
        sqlx::query(
            "INSERT INTO routing_runtime_generation (
                 runtime_generation_id, policy_generation_id,
                 quality_generation_id, circuit_generation_id,
                 policy_revision, quality_policy_revision,
                 circuit_policy_revision, algorithm_version, status,
                 input_observation_watermark, input_circuit_event_watermark,
                 policy_input_hash, quality_input_hash, circuit_input_hash,
                 policy_content_hash, quality_content_hash, circuit_content_hash,
                 checkpoint_ref, ready_at_ms, created_at_ms, updated_at_ms
             ) VALUES (
                 'runtime-old', ?1, 'quality-old', 'circuit-old',
                 ?2, ?2, ?2, '1', 'ready', 0, 0,
                 ?3, ?3, ?3, ?3, ?3, ?3, 'checkpoint-old', 20, 20, 20
             )",
        )
        .bind(&identity.1)
        .bind(identity.0)
        .bind(&hash)
        .execute(write.connection())
        .await
        .expect("old ready runtime fixture");
        sqlx::query(
            "INSERT INTO routing_runtime_generation (
                 runtime_generation_id, policy_generation_id,
                 quality_generation_id, circuit_generation_id,
                 policy_revision, quality_policy_revision,
                 circuit_policy_revision, algorithm_version, status,
                 failure_code, failed_at_ms, created_at_ms, updated_at_ms
             ) VALUES (
                 'runtime-new', ?1, 'quality-new', 'circuit-new',
                 ?2, ?2, ?2, '1', 'failed',
                 'superseded_by_input_tail', 30, 30, 30
             )",
        )
        .bind(&identity.1)
        .bind(identity.0)
        .execute(write.connection())
        .await
        .expect("new failed runtime fixture");
        write.commit().await.expect("commit runtime fixtures");

        let mut read = runtime.handle().begin_read().await.expect("read latest");
        let latest = load_publication_by_revision(read.connection(), revision)
            .await
            .expect("latest publication query")
            .expect("latest publication");
        assert_eq!(latest.runtime_status.as_deref(), Some("failed"));
        assert_eq!(
            latest.runtime_failure_code.as_deref(),
            Some("superseded_by_input_tail")
        );
        assert_eq!(latest.runtime_updated_at_ms, Some(30));
        drop(read);
        runtime.close().await.expect("close");
    }
}
