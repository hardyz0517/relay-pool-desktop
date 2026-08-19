use serde_json::Value;
use sqlx::{Row, SqliteConnection};
use std::collections::BTreeMap;

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingQualityStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingCheckpointCursor {
    pub(crate) sequence: u64,
    pub(crate) item_id: Option<String>,
}

impl RoutingQualityStore {
    pub(crate) async fn load_summary_json(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<BTreeMap<String, Value>, PersistenceError> {
        let mut result = BTreeMap::new();
        for scope in scopes.iter().take(1024) {
            let row =
                sqlx::query("SELECT summary_json FROM routing_quality_summaries WHERE scope = ?1")
                    .bind(scope)
                    .fetch_optional(&mut *connection)
                    .await?;
            if let Some(row) = row {
                let json = row.get::<String, _>("summary_json");
                let value = serde_json::from_str(&json).map_err(|error| {
                    PersistenceError::InvariantViolation(format!(
                        "routing quality summary is invalid: {error}"
                    ))
                })?;
                result.insert(scope.clone(), value);
            }
        }
        Ok(result)
    }

    pub(crate) async fn list_summary_scopes(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<String>, PersistenceError> {
        let rows =
            sqlx::query("SELECT scope FROM routing_quality_summaries ORDER BY scope LIMIT 1024")
                .fetch_all(&mut *connection)
                .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("scope"))
            .collect())
    }

    pub async fn load_health_axes(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<BTreeMap<String, BTreeMap<String, u16>>, PersistenceError> {
        if scopes.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut result = BTreeMap::new();
        for scope in scopes.iter().take(1024) {
            if scope.is_empty() {
                return Err(PersistenceError::ConstraintViolation);
            }
            let rows = sqlx::query(
                "SELECT axis, value_basis_points FROM routing_health_axes WHERE scope = ?1",
            )
            .bind(scope)
            .fetch_all(&mut *connection)
            .await?;
            let axes = rows
                .into_iter()
                .map(|row| {
                    let value = row.get::<i64, _>("value_basis_points");
                    if !(0..=10_000).contains(&value) {
                        return Err(PersistenceError::InvariantViolation(
                            "routing health axis is outside basis-point range".into(),
                        ));
                    }
                    Ok((row.get::<String, _>("axis"), value as u16))
                })
                .collect::<Result<BTreeMap<_, _>, PersistenceError>>()?;
            result.insert(scope.clone(), axes);
        }
        Ok(result)
    }

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

    pub(crate) async fn load_checkpoint_cursor(
        &self,
        connection: &mut SqliteConnection,
        projector: &str,
        projector_version: &str,
        scope: &str,
    ) -> Result<Option<RoutingCheckpointCursor>, PersistenceError> {
        validate_scope(scope, 1, 0)?;
        let row = sqlx::query(
            "SELECT checkpoint_sequence, error_code FROM routing_projector_checkpoints WHERE projector = ?1 AND projector_version = ?2 AND scope = ?3",
        )
        .bind(projector)
        .bind(projector_version)
        .bind(scope)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            let sequence =
                u64::try_from(row.get::<i64, _>("checkpoint_sequence")).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "stored projector checkpoint is negative".into(),
                    )
                })?;
            Ok(RoutingCheckpointCursor {
                sequence,
                item_id: row.get("error_code"),
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
