use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::models::routing_generation::RoutingCutoverMode;
use crate::models::routing_policy::RoutingPolicyConfigV3;
use crate::persistence::error::PersistenceError;
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
