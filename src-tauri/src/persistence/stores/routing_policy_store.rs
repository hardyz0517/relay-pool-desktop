use serde_json::Value;
use sqlx::{Connection, Row, SqliteConnection};

use crate::models::routing_generation::RoutingCutoverMode;
use crate::models::routing_policy::{
    RoutingPolicyConfigV1, RoutingPolicyConfigV2, RoutingPolicyConfigV3,
};
use crate::persistence::error::PersistenceError;
use crate::persistence::stores::domain_revision_store::DomainRevisionStore;
use crate::persistence::stores::routing_generation_store::RoutingGenerationStore;

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct CircuitPolicyParameters {
    pub(crate) policy_revision: u64,
    pub(crate) consecutive_failure_threshold: u16,
    pub(crate) recovery_success_threshold: u16,
    pub(crate) recovery_wait_ms: u64,
}

impl RoutingPolicyStore {
    /// Loads the circuit policy from the same active-generation boundary used
    /// by routing. Keeping this read in the persistence store prevents
    /// lifecycle/application modules from reaching directly into SQLite and
    /// avoids applying stale legacy thresholds after a v3 cutover.
    pub(crate) async fn load_circuit_policy_parameters(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<CircuitPolicyParameters, PersistenceError> {
        let defaults = CircuitPolicyParameters {
            policy_revision: 1,
            consecutive_failure_threshold:
                crate::models::routing_policy::DEFAULT_CONSECUTIVE_FAILURE_THRESHOLD,
            recovery_success_threshold: u16::from(
                crate::models::routing_policy::DEFAULT_RECOVERY_SUCCESS_THRESHOLD,
            ),
            recovery_wait_ms: u64::from(
                crate::models::routing_policy::DEFAULT_RECOVERY_WAIT_SECONDS,
            ) * 1_000,
        };
        let registry = RoutingGenerationStore
            .load_registry_snapshot(connection)
            .await?;
        let stored = match registry.marker.mode {
            RoutingCutoverMode::PreCutover => self.load(connection).await?,
            RoutingCutoverMode::V3Active => {
                let active = registry.active.ok_or_else(|| {
                    PersistenceError::InvariantViolation(
                        "routing_generation_registry_corrupt: active policy is missing".into(),
                    )
                })?;
                let row = sqlx::query(
                    "SELECT config_json, target_policy_revision, status
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
                Some(StoredRoutingPolicy {
                    config: serde_json::from_str(&row.get::<String, _>("config_json"))
                        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
                    revision: u64::try_from(row.get::<i64, _>("target_policy_revision"))
                        .map_err(|_| PersistenceError::ConstraintViolation)?,
                    policy_version: "routing-policy-v3".to_string(),
                    system_version: String::new(),
                    status: "active".to_string(),
                    updated_at_ms: 0,
                })
            }
        };
        let Some(stored) = stored else {
            return Ok(defaults);
        };
        let Ok(policy) = RoutingPolicyConfigV3::from_stored_value(&stored.config) else {
            return Ok(CircuitPolicyParameters {
                policy_revision: stored.revision,
                ..defaults
            });
        };
        Ok(CircuitPolicyParameters {
            policy_revision: stored.revision,
            consecutive_failure_threshold: policy.retry.consecutive_failure_threshold,
            recovery_success_threshold: u16::from(
                policy.circuit_breaker.recovery_success_threshold,
            ),
            recovery_wait_ms: u64::from(policy.circuit_breaker.recovery_wait_seconds) * 1_000,
        })
    }

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

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-routing-policy-cas; owner=persistence/stores/routing_policy_store; remove_when=v3 policy staging coordinator is the sole mutation path"
        )
    )]
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

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-routing-policy-validation; owner=persistence/stores/routing_policy_store; remove_when=v3 staged policy validation is the sole persistence boundary"
    )
)]
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
    match config.get("version").and_then(Value::as_u64) {
        Some(3) => {
            let typed = serde_json::from_value::<RoutingPolicyConfigV3>(config.clone())
                .map_err(|_| PersistenceError::ConstraintViolation)?;
            typed
                .validate()
                .map_err(|_| PersistenceError::ConstraintViolation)?;
        }
        Some(2) => {
            let typed = serde_json::from_value::<RoutingPolicyConfigV2>(config.clone())
                .map_err(|_| PersistenceError::ConstraintViolation)?;
            typed
                .validate()
                .map_err(|_| PersistenceError::ConstraintViolation)?;
        }
        Some(1) => {
            let typed = serde_json::from_value::<RoutingPolicyConfigV1>(config.clone())
                .map_err(|_| PersistenceError::ConstraintViolation)?;
            typed
                .validate()
                .map_err(|_| PersistenceError::ConstraintViolation)?;
        }
        _ => return Err(PersistenceError::ConstraintViolation),
    }
    Ok(())
}
