use serde_json::Value;
use sqlx::{Connection, Row, SqliteConnection};

use crate::models::routing_policy::{RoutingPolicyConfigV1, RoutingPolicyConfigV2};
use crate::persistence::error::PersistenceError;
use crate::persistence::stores::domain_revision_store::DomainRevisionStore;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredRoutingPolicy {
    pub(crate) config: Value,
    pub(crate) revision: u64,
    pub(crate) policy_version: String,
    pub(crate) system_version: String,
    pub(crate) status: String,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingPolicyStore;

impl RoutingPolicyStore {
    pub(crate) async fn load(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
        sqlx::query(
            "SELECT config_json, config_revision, policy_version, system_version, status, updated_at_ms FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_optional(&mut *connection)
        .await?
        .map(policy_from_row)
        .transpose()
    }

    pub(crate) async fn save_compare_and_swap(
        &self,
        connection: &mut SqliteConnection,
        expected_revision: Option<u64>,
        config: &Value,
        policy_version: &str,
        system_version: &str,
        status: &str,
        now_ms: i64,
    ) -> Result<StoredRoutingPolicy, PersistenceError> {
        validate_policy_input(config, policy_version, system_version, status, now_ms)?;
        let config_json = serde_json::to_string(config)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let mut transaction = connection.begin().await?;
        let revisions = DomainRevisionStore;
        let baseline = revisions.load(&mut *transaction, "routing_policy").await?;
        if expected_revision.is_some_and(|expected| expected != baseline.revision) {
            return Err(PersistenceError::RevisionConflict("routing_policy".into()));
        }
        if expected_revision.is_none()
            && sqlx::query("SELECT 1 FROM routing_policy WHERE singleton_key = 1")
                .fetch_optional(&mut *transaction)
                .await?
                .is_some()
        {
            return Err(PersistenceError::RevisionConflict("routing_policy".into()));
        }
        if let Some(expected_revision) = expected_revision {
            if let Some(existing) = sqlx::query(
                "SELECT config_json, policy_version, system_version, status FROM routing_policy WHERE singleton_key = 1 AND config_revision = ?1",
            )
            .bind(i64::try_from(expected_revision).map_err(|_| PersistenceError::InvariantViolation("routing policy expected revision exceeds SQLite range".into()))?)
            .fetch_optional(&mut *transaction)
            .await?
            {
                let existing_config: Value = serde_json::from_str(existing.get::<String, _>("config_json").as_str())
                    .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
                if existing_config == *config
                    && existing.get::<String, _>("policy_version") == policy_version
                    && existing.get::<String, _>("system_version") == system_version
                    && existing.get::<String, _>("status") == status
                {
                    transaction.rollback().await?;
                    return self.load(connection).await?.ok_or_else(|| {
                        PersistenceError::InvariantViolation("routing policy disappeared during no-op CAS".into())
                    });
                }
            }
        }
        let advanced = revisions
            .advance(
                &mut *transaction,
                "routing_policy",
                baseline.revision,
                now_ms,
            )
            .await?;
        let next_revision = i64::try_from(advanced.revision).map_err(|_| {
            PersistenceError::InvariantViolation(
                "routing policy revision exceeds SQLite range".into(),
            )
        })?;

        let changed = match expected_revision {
            Some(expected_revision) => sqlx::query(
                "UPDATE routing_policy SET config_json = ?1, config_revision = ?2, policy_version = ?3, system_version = ?4, status = ?5, updated_at_ms = ?6 WHERE singleton_key = 1 AND config_revision = ?7",
            )
            .bind(&config_json)
            .bind(next_revision)
            .bind(policy_version)
            .bind(system_version)
            .bind(status)
            .bind(now_ms)
            .bind(i64::try_from(expected_revision).map_err(|_| PersistenceError::InvariantViolation("routing policy expected revision exceeds SQLite range".into()))?)
            .execute(&mut *transaction)
            .await?
            .rows_affected(),
            None => sqlx::query(
                "INSERT INTO routing_policy (singleton_key, config_json, config_revision, policy_version, system_version, status, created_at_ms, updated_at_ms) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?6) ON CONFLICT(singleton_key) DO NOTHING",
            )
            .bind(&config_json)
            .bind(next_revision)
            .bind(policy_version)
            .bind(system_version)
            .bind(status)
            .bind(now_ms)
            .execute(&mut *transaction)
            .await?
            .rows_affected(),
        };
        if changed != 1 {
            return Err(PersistenceError::RevisionConflict("routing_policy".into()));
        }
        sqlx::query(
            "INSERT INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(next_revision)
        .bind(config_json)
        .bind(policy_version)
        .bind(system_version)
        .bind(status)
        .bind(now_ms)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.load(connection).await?.ok_or_else(|| {
            PersistenceError::InvariantViolation("routing policy disappeared after CAS".into())
        })
    }
}

fn policy_from_row(row: sqlx::sqlite::SqliteRow) -> Result<StoredRoutingPolicy, PersistenceError> {
    let revision = row.get::<i64, _>("config_revision");
    if revision <= 0 {
        return Err(PersistenceError::InvariantViolation(
            "routing policy revision is invalid".into(),
        ));
    }
    let config_json: String = row.get("config_json");
    Ok(StoredRoutingPolicy {
        config: serde_json::from_str(&config_json)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
        revision: revision as u64,
        policy_version: row.get("policy_version"),
        system_version: row.get("system_version"),
        status: row.get("status"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

fn validate_policy_input(
    config: &Value,
    policy_version: &str,
    system_version: &str,
    status: &str,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    if !config.is_object()
        || policy_version.is_empty()
        || policy_version.len() > 96
        || system_version.is_empty()
        || system_version.len() > 96
        || !matches!(
            status,
            "routing_configuration_required" | "active" | "invalid"
        )
        || now_ms < 0
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    if config.get("version").and_then(Value::as_u64) == Some(2) {
        let typed = serde_json::from_value::<RoutingPolicyConfigV2>(config.clone())
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        typed
            .validate()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
    } else {
        let typed = serde_json::from_value::<RoutingPolicyConfigV1>(config.clone())
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        typed
            .validate()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
    }
    Ok(())
}
