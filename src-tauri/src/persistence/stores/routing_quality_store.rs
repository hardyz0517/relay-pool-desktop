use serde_json::Value;
use sqlx::SqliteConnection;

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingQualityStore;

impl RoutingQualityStore {
    pub(crate) async fn save_summary(
        &self,
        connection: &mut SqliteConnection,
        scope: &str,
        revision: u64,
        summary: &Value,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_scope(scope, revision, now_ms)?;
        let json = serde_json::to_string(summary)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        sqlx::query(
            "INSERT INTO routing_quality_summaries (scope, quality_revision, summary_json, updated_at_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(scope) DO UPDATE SET quality_revision = excluded.quality_revision, summary_json = excluded.summary_json, updated_at_ms = excluded.updated_at_ms WHERE excluded.quality_revision >= routing_quality_summaries.quality_revision",
        )
        .bind(scope)
        .bind(i64::try_from(revision).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(json)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn save_health_axis(
        &self,
        connection: &mut SqliteConnection,
        scope: &str,
        axis: &str,
        revision: u64,
        value_basis_points: u16,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_scope(scope, revision, now_ms)?;
        if !matches!(
            axis,
            "availability" | "latency" | "reliability" | "freshness"
        ) {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query(
            "INSERT INTO routing_health_axes (scope, axis, health_revision, value_basis_points, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(scope, axis) DO UPDATE SET health_revision = excluded.health_revision, value_basis_points = excluded.value_basis_points, updated_at_ms = excluded.updated_at_ms WHERE excluded.health_revision >= routing_health_axes.health_revision",
        )
        .bind(scope)
        .bind(axis)
        .bind(i64::try_from(revision).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(i64::from(value_basis_points))
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn save_checkpoint(
        &self,
        connection: &mut SqliteConnection,
        projector: &str,
        projector_version: &str,
        scope: &str,
        checkpoint_sequence: u64,
        status: &str,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if projector.is_empty()
            || projector_version.is_empty()
            || !matches!(status, "ready" | "projecting" | "failed")
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        validate_scope(scope, checkpoint_sequence.max(1), now_ms)?;
        sqlx::query(
            "INSERT INTO routing_projector_checkpoints (projector, projector_version, scope, checkpoint_sequence, status, error_code, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(projector, projector_version, scope) DO UPDATE SET
                 checkpoint_sequence = excluded.checkpoint_sequence,
                 status = excluded.status,
                 error_code = excluded.error_code,
                 updated_at_ms = excluded.updated_at_ms
             WHERE excluded.checkpoint_sequence >= routing_projector_checkpoints.checkpoint_sequence",
        )
        .bind(projector)
        .bind(projector_version)
        .bind(scope)
        .bind(i64::try_from(checkpoint_sequence).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(status)
        .bind(error_code)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn load_checkpoint(
        &self,
        connection: &mut SqliteConnection,
        projector: &str,
        projector_version: &str,
        scope: &str,
    ) -> Result<Option<u64>, PersistenceError> {
        validate_scope(scope, 1, 0)?;
        let value = sqlx::query_scalar::<_, i64>(
            "SELECT checkpoint_sequence FROM routing_projector_checkpoints WHERE projector = ?1 AND projector_version = ?2 AND scope = ?3",
        )
        .bind(projector)
        .bind(projector_version)
        .bind(scope)
        .fetch_optional(&mut *connection)
        .await?;
        value
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "stored projector checkpoint is negative".into(),
                    )
                })
            })
            .transpose()
    }
}

fn validate_scope(scope: &str, revision: u64, now_ms: i64) -> Result<(), PersistenceError> {
    if scope.is_empty()
        || scope.len() > 192
        || scope.chars().any(char::is_control)
        || revision == 0
        || now_ms < 0
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}
