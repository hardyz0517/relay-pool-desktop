use serde_json::Value;
use sqlx::{Row, SqliteConnection};
use std::collections::{BTreeMap, BTreeSet};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingQualityStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingCheckpointCursor {
    pub(crate) sequence: u64,
    pub(crate) item_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RoutingAttemptCountDiagnostics {
    pub(crate) raw_attempt_count: u64,
    pub(crate) deduplicated_request_count: u64,
}

pub(crate) const MAX_ACTIVE_QUALITY_LAG_SECONDS: u64 = 900;
pub(crate) const MAX_PROJECTOR_BACKLOG: u64 = 100_000;
pub(crate) const ROUTING_QUALITY_PROJECTOR_ID: &str = "routing-projection-v1";
pub(crate) const ROUTING_QUALITY_CURSOR_SCOPE: &str = "__routing_projection_ingestion_cursor__";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RoutingQualityPlanningRead {
    pub(crate) axes: BTreeMap<String, BTreeMap<String, u16>>,
    pub(crate) unavailable_scopes: BTreeSet<String>,
    pub(crate) quality_revision: u64,
    pub(crate) health_revision: u64,
    pub(crate) projection_backlog: u64,
    pub(crate) projection_lag_seconds: u64,
    pub(crate) quality_stale: bool,
    pub(crate) quality_available: bool,
}

impl RoutingQualityStore {
    /// Capture the active quality read model and its freshness evidence in the
    /// caller-owned read transaction. Missing summaries are valid for newly
    /// added keys and use the configured optimistic values; corrupt or
    /// explicitly unavailable summaries do not.
    pub(crate) async fn load_planning_read(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: Option<&str>,
        scopes: &[String],
        now_ms: i64,
    ) -> Result<RoutingQualityPlanningRead, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let Some(quality_generation_id) = quality_generation_id else {
            return Ok(RoutingQualityPlanningRead {
                quality_available: true,
                ..RoutingQualityPlanningRead::default()
            });
        };
        validate_generation_id(quality_generation_id)?;

        let generation = sqlx::query(
            "SELECT input_observation_watermark
             FROM routing_quality_generation_v3
             WHERE quality_generation_id = ?1 AND status = 'active'",
        )
        .bind(quality_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| generation_registry_corrupt("active quality component is missing"))?;
        let base_watermark = to_u64_nonnegative(
            generation.get::<Option<i64>, _>("input_observation_watermark"),
            "active quality watermark",
        )?
        .unwrap_or(0);

        let revisions = sqlx::query(
            "SELECT
                 (SELECT MAX(quality_revision) FROM routing_quality_summary_v3
                  WHERE quality_generation_id = ?1) AS quality_revision,
                 (SELECT MAX(health_revision) FROM routing_quality_health_axis_v3
                  WHERE quality_generation_id = ?1) AS health_revision",
        )
        .bind(quality_generation_id)
        .fetch_one(&mut *connection)
        .await?;
        let stored_quality_revision = to_u64_nonnegative(
            revisions.get::<Option<i64>, _>("quality_revision"),
            "quality revision",
        )?
        .unwrap_or(0);
        let stored_health_revision = to_u64_nonnegative(
            revisions.get::<Option<i64>, _>("health_revision"),
            "health revision",
        )?
        .unwrap_or(0);
        let quality_revision = stored_quality_revision.max(base_watermark).max(1);
        let health_revision = stored_health_revision.max(quality_revision).max(1);

        let checkpoint = sqlx::query_scalar::<_, i64>(
            "SELECT checkpoint_sequence
             FROM routing_quality_incremental_checkpoint_v3
             WHERE quality_generation_id = ?1 AND projector = ?2
               AND projector_version = 'routing_quality_v3' AND scope = ?3",
        )
        .bind(quality_generation_id)
        .bind(ROUTING_QUALITY_PROJECTOR_ID)
        .bind(ROUTING_QUALITY_CURSOR_SCOPE)
        .fetch_optional(&mut *connection)
        .await?
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                PersistenceError::InvariantViolation(
                    "routing quality checkpoint is negative".into(),
                )
            })
        })
        .transpose()?
        .unwrap_or(base_watermark)
        .max(base_watermark);
        let checkpoint_i64 =
            i64::try_from(checkpoint).map_err(|_| PersistenceError::ConstraintViolation)?;
        let backlog = sqlx::query(
            "SELECT COUNT(*) AS backlog, MIN(ingested_at_ms) AS oldest_ingested_at_ms
             FROM routing_observations
             WHERE generation_eligibility = 'active'
               AND ingestion_sequence IS NOT NULL AND ingestion_sequence > ?1",
        )
        .bind(checkpoint_i64)
        .fetch_one(&mut *connection)
        .await?;
        let projection_backlog = u64::try_from(backlog.get::<i64, _>("backlog"))
            .map_err(|_| PersistenceError::InvariantViolation("negative quality backlog".into()))?;
        let lag_ms = backlog
            .get::<Option<i64>, _>("oldest_ingested_at_ms")
            .map(|oldest| now_ms.saturating_sub(oldest).max(0) as u64)
            .unwrap_or(0);
        let projection_lag_seconds = lag_ms.saturating_add(999) / 1_000;
        let quality_stale = projection_backlog > 0;
        let quality_available = projection_lag_seconds <= MAX_ACTIVE_QUALITY_LAG_SECONDS;

        let mut result = RoutingQualityPlanningRead {
            axes: BTreeMap::new(),
            unavailable_scopes: BTreeSet::new(),
            quality_revision,
            health_revision,
            projection_backlog,
            projection_lag_seconds,
            quality_stale,
            quality_available,
        };
        let mut requested_scopes = scopes
            .iter()
            .filter(|scope| !scope.is_empty())
            .take(1024)
            .cloned()
            .collect::<Vec<_>>();
        requested_scopes.sort();
        requested_scopes.dedup();
        if requested_scopes.is_empty() {
            return Ok(result);
        }
        let requested_json = serde_json::to_string(&requested_scopes)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let rows = sqlx::query(
            "WITH requested(scope) AS (SELECT value FROM json_each(?2)),
                  latest(scope, lifecycle_revision) AS (
                    SELECT summary.scope, MAX(summary.station_key_lifecycle_revision)
                    FROM routing_quality_summary_v3 summary
                    JOIN requested ON requested.scope = summary.scope
                    WHERE summary.quality_generation_id = ?1
                    GROUP BY summary.scope
                  )
             SELECT summary.scope, summary.summary_json, axis.axis,
                    axis.value_basis_points
             FROM latest
             JOIN routing_quality_summary_v3 summary
               ON summary.quality_generation_id = ?1
              AND summary.scope = latest.scope
              AND summary.station_key_lifecycle_revision = latest.lifecycle_revision
             LEFT JOIN routing_quality_health_axis_v3 axis
               ON axis.quality_generation_id = summary.quality_generation_id
              AND axis.scope = summary.scope
              AND axis.station_key_lifecycle_revision = summary.station_key_lifecycle_revision
             ORDER BY summary.scope ASC, axis.axis ASC",
        )
        .bind(quality_generation_id)
        .bind(requested_json)
        .fetch_all(&mut *connection)
        .await?;
        let mut parsed_summaries = BTreeMap::<String, bool>::new();
        for row in rows {
            let scope = row.get::<String, _>("scope");
            let summary_available = *parsed_summaries.entry(scope.clone()).or_insert_with(|| {
                serde_json::from_str::<Value>(&row.get::<String, _>("summary_json"))
                    .ok()
                    .filter(|summary| {
                        summary.get("projector_version").and_then(Value::as_str)
                            == Some("routing_quality_v3")
                    })
                    .and_then(|summary| summary.get("quality_unavailable").and_then(Value::as_bool))
                    == Some(false)
            });
            if !summary_available {
                result.unavailable_scopes.insert(scope);
                continue;
            }
            let Some(axis) = row.get::<Option<String>, _>("axis") else {
                continue;
            };
            let value = row
                .get::<Option<i64>, _>("value_basis_points")
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation("quality axis has no value".into())
                })?;
            if !(0..=10_000).contains(&value) {
                return Err(PersistenceError::InvariantViolation(
                    "routing generation health axis is outside basis-point range".into(),
                ));
            }
            result
                .axes
                .entry(scope)
                .or_default()
                .insert(axis, value as u16);
        }
        for (scope, available) in parsed_summaries {
            if available
                && !result.axes.get(&scope).is_some_and(|axes| {
                    axes.contains_key("reliability") && axes.contains_key("latency")
                })
            {
                result.axes.remove(&scope);
                result.unavailable_scopes.insert(scope);
            }
        }
        Ok(result)
    }

    pub(crate) async fn load_active_generation_id(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<String>, PersistenceError> {
        let marker = sqlx::query(
            "SELECT status, runtime_generation_id
             FROM routing_runtime_cutover_marker WHERE singleton_key = 1",
        )
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| generation_registry_corrupt("cutover marker is missing"))?;
        let status = marker.get::<String, _>("status");
        let runtime_generation_id = marker.get::<Option<String>, _>("runtime_generation_id");
        match status.as_str() {
            "pre_cutover" if runtime_generation_id.is_none() => Ok(None),
            "v3_active" => {
                let runtime_generation_id = runtime_generation_id.ok_or_else(|| {
                    generation_registry_corrupt("v3 marker has no runtime generation")
                })?;
                let row = sqlx::query(
                    "SELECT quality_generation_id FROM routing_runtime_generation
                     WHERE runtime_generation_id = ?1 AND status = 'active'",
                )
                .bind(runtime_generation_id)
                .fetch_optional(&mut *connection)
                .await?
                .ok_or_else(|| {
                    generation_registry_corrupt("active quality generation is missing")
                })?;
                Ok(Some(row.get("quality_generation_id")))
            }
            _ => Err(generation_registry_corrupt(
                "cutover marker and active pointer disagree",
            )),
        }
    }

    pub(crate) async fn load_generation_summary_json(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
        scopes: &[String],
    ) -> Result<BTreeMap<String, Value>, PersistenceError> {
        validate_generation_id(quality_generation_id)?;
        let mut result = BTreeMap::new();
        for scope in scopes.iter().take(1024) {
            let row = sqlx::query(
                "SELECT summary_json FROM routing_quality_summary_v3
                 WHERE quality_generation_id = ?1 AND scope = ?2
                 ORDER BY station_key_lifecycle_revision DESC LIMIT 1",
            )
            .bind(quality_generation_id)
            .bind(scope)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(row) = row {
                let json = row.get::<String, _>("summary_json");
                let value = serde_json::from_str(&json).map_err(|error| {
                    PersistenceError::InvariantViolation(format!(
                        "routing generation quality summary is invalid: {error}"
                    ))
                })?;
                result.insert(scope.clone(), value);
            }
        }
        Ok(result)
    }

    pub(crate) async fn list_generation_summary_scopes(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
    ) -> Result<Vec<String>, PersistenceError> {
        validate_generation_id(quality_generation_id)?;
        let rows = sqlx::query(
            "SELECT scope FROM routing_quality_summary_v3
             WHERE quality_generation_id = ?1 ORDER BY scope LIMIT 1024",
        )
        .bind(quality_generation_id)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("scope"))
            .collect())
    }

    pub(crate) async fn load_summary_json(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<BTreeMap<String, Value>, PersistenceError> {
        if let Some(generation_id) = self.load_active_generation_id(connection).await? {
            return self
                .load_generation_summary_json(connection, &generation_id, scopes)
                .await;
        }
        let mut result = BTreeMap::new();
        for scope in scopes.iter().take(1024) {
            let row = sqlx::query(
                "SELECT summary_json FROM routing_quality_summaries WHERE scope = ?1 AND json_extract(summary_json, '$.projector_version') = 'routing_quality_v3'",
            )
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

    pub(crate) async fn load_attempt_count_diagnostics(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<BTreeMap<String, RoutingAttemptCountDiagnostics>, PersistenceError> {
        let mut result = BTreeMap::new();
        for scope in scopes.iter().take(1024) {
            let station_key_id = scope
                .strip_prefix("station_key:")
                .filter(|value| !value.is_empty())
                .ok_or(PersistenceError::ConstraintViolation)?;
            let row = sqlx::query(
                "SELECT COALESCE(SUM(expected_attempt_count), 0) AS raw_attempt_count,
                        COUNT(*) AS deduplicated_request_count
                 FROM routing_attempt_cluster_v3
                 WHERE source = 'real_request' AND station_key_id = ?1
                   AND cluster_finalized = 1 AND generation_eligibility = 'active'",
            )
            .bind(station_key_id)
            .fetch_one(&mut *connection)
            .await?;
            let raw_attempt_count = row.get::<i64, _>("raw_attempt_count");
            let deduplicated_request_count = row.get::<i64, _>("deduplicated_request_count");
            result.insert(
                scope.clone(),
                RoutingAttemptCountDiagnostics {
                    raw_attempt_count: u64::try_from(raw_attempt_count)
                        .map_err(|_| PersistenceError::ConstraintViolation)?,
                    deduplicated_request_count: u64::try_from(deduplicated_request_count)
                        .map_err(|_| PersistenceError::ConstraintViolation)?,
                },
            );
        }
        Ok(result)
    }

    pub(crate) async fn list_summary_scopes(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<String>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT scope FROM routing_quality_summaries WHERE json_extract(summary_json, '$.projector_version') = 'routing_quality_v3' ORDER BY scope LIMIT 1024",
        )
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("scope"))
            .collect())
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
        if summary.get("projector_version").and_then(Value::as_str) != Some("routing_quality_v3") {
            return Err(PersistenceError::InvariantViolation(
                "routing quality store accepts only v3 summaries".into(),
            ));
        }
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

    pub(crate) async fn save_generation_summary(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
        scope: &str,
        station_key_id: &str,
        station_key_lifecycle_revision: u64,
        revision: u64,
        summary: &Value,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_generation_output(
            quality_generation_id,
            scope,
            station_key_id,
            station_key_lifecycle_revision,
            revision,
            now_ms,
        )?;
        if summary.get("projector_version").and_then(Value::as_str) != Some("routing_quality_v3")
            || summary.get("scope").and_then(Value::as_str) != Some(scope)
        {
            return Err(PersistenceError::InvariantViolation(
                "routing generation quality store accepts only matching v3 summaries".into(),
            ));
        }
        let json = serde_json::to_string(summary)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        sqlx::query(
            "INSERT INTO routing_quality_summary_v3 (
                 quality_generation_id, scope, station_key_id,
                 station_key_lifecycle_revision, quality_revision,
                 summary_json, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(quality_generation_id, scope, station_key_lifecycle_revision)
             DO UPDATE SET quality_revision = excluded.quality_revision,
                 summary_json = excluded.summary_json,
                 updated_at_ms = excluded.updated_at_ms
             WHERE excluded.quality_revision >= routing_quality_summary_v3.quality_revision",
        )
        .bind(quality_generation_id)
        .bind(scope)
        .bind(station_key_id)
        .bind(to_i64(station_key_lifecycle_revision)?)
        .bind(to_i64(revision)?)
        .bind(json)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn save_generation_health_axis(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
        scope: &str,
        station_key_id: &str,
        station_key_lifecycle_revision: u64,
        axis: &str,
        revision: u64,
        value_basis_points: u16,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_generation_output(
            quality_generation_id,
            scope,
            station_key_id,
            station_key_lifecycle_revision,
            revision,
            now_ms,
        )?;
        if !matches!(
            axis,
            "availability" | "latency" | "reliability" | "freshness"
        ) || value_basis_points > 10_000
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query(
            "INSERT INTO routing_quality_health_axis_v3 (
                 quality_generation_id, scope, station_key_id,
                 station_key_lifecycle_revision, axis, health_revision,
                 value_basis_points, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(
                 quality_generation_id, scope,
                 station_key_lifecycle_revision, axis
             ) DO UPDATE SET health_revision = excluded.health_revision,
                 value_basis_points = excluded.value_basis_points,
                 updated_at_ms = excluded.updated_at_ms
             WHERE excluded.health_revision >= routing_quality_health_axis_v3.health_revision",
        )
        .bind(quality_generation_id)
        .bind(scope)
        .bind(station_key_id)
        .bind(to_i64(station_key_lifecycle_revision)?)
        .bind(axis)
        .bind(to_i64(revision)?)
        .bind(i64::from(value_basis_points))
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn save_generation_checkpoint(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
        projector: &str,
        projector_version: &str,
        scope: &str,
        checkpoint_sequence: u64,
        status: &str,
        cursor_item_id: Option<&str>,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_generation_id(quality_generation_id)?;
        if projector.is_empty()
            || projector.len() > 96
            || projector_version.is_empty()
            || projector_version.len() > 96
            || !matches!(status, "ready" | "projecting" | "failed")
            || cursor_item_id.is_some_and(|value| value.is_empty() || value.len() > 192)
            || error_code.is_some_and(|value| value.is_empty() || value.len() > 96)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        validate_scope(scope, checkpoint_sequence.max(1), now_ms)?;
        sqlx::query(
            "INSERT INTO routing_quality_incremental_checkpoint_v3 (
                 quality_generation_id, projector, projector_version, scope,
                 checkpoint_sequence, status, cursor_item_id, error_code, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(quality_generation_id, projector, projector_version, scope)
             DO UPDATE SET checkpoint_sequence = excluded.checkpoint_sequence,
                 status = excluded.status, cursor_item_id = excluded.cursor_item_id,
                 error_code = excluded.error_code, updated_at_ms = excluded.updated_at_ms
             WHERE excluded.checkpoint_sequence >= routing_quality_incremental_checkpoint_v3.checkpoint_sequence",
        )
        .bind(quality_generation_id)
        .bind(projector)
        .bind(projector_version)
        .bind(scope)
        .bind(to_i64(checkpoint_sequence)?)
        .bind(status)
        .bind(cursor_item_id)
        .bind(error_code)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn load_generation_checkpoint_cursor(
        &self,
        connection: &mut SqliteConnection,
        quality_generation_id: &str,
        projector: &str,
        projector_version: &str,
        scope: &str,
    ) -> Result<Option<RoutingCheckpointCursor>, PersistenceError> {
        validate_generation_id(quality_generation_id)?;
        validate_scope(scope, 1, 0)?;
        let row = sqlx::query(
            "SELECT checkpoint_sequence, cursor_item_id
             FROM routing_quality_incremental_checkpoint_v3
             WHERE quality_generation_id = ?1 AND projector = ?2
               AND projector_version = ?3 AND scope = ?4",
        )
        .bind(quality_generation_id)
        .bind(projector)
        .bind(projector_version)
        .bind(scope)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            Ok(RoutingCheckpointCursor {
                sequence: u64::try_from(row.get::<i64, _>("checkpoint_sequence")).map_err(
                    |_| {
                        PersistenceError::InvariantViolation(
                            "generation projector checkpoint is negative".into(),
                        )
                    },
                )?,
                item_id: row.get("cursor_item_id"),
            })
        })
        .transpose()
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

fn validate_generation_id(quality_generation_id: &str) -> Result<(), PersistenceError> {
    if quality_generation_id.len() < 5
        || quality_generation_id.len() > 192
        || quality_generation_id.chars().any(char::is_control)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn generation_registry_corrupt(detail: &str) -> PersistenceError {
    PersistenceError::InvariantViolation(format!("routing_generation_registry_corrupt: {detail}"))
}

fn to_u64_nonnegative(value: Option<i64>, field: &str) -> Result<Option<u64>, PersistenceError> {
    value
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| PersistenceError::InvariantViolation(format!("{field} is negative")))
        })
        .transpose()
}

fn validate_generation_output(
    quality_generation_id: &str,
    scope: &str,
    station_key_id: &str,
    station_key_lifecycle_revision: u64,
    revision: u64,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    validate_generation_id(quality_generation_id)?;
    validate_scope(scope, revision, now_ms)?;
    if station_key_id.is_empty()
        || station_key_id.len() > 160
        || station_key_id.chars().any(char::is_control)
        || station_key_lifecycle_revision == 0
        || scope != format!("station_key:{station_key_id}")
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
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

#[cfg(test)]
mod tests {
    use sqlx::Executor;

    use crate::persistence::runtime::PersistenceRuntime;

    use super::{RoutingQualityStore, MAX_ACTIVE_QUALITY_LAG_SECONDS};

    const GENERATION_ID: &str = "quality-generation-test";
    const SCOPE: &str = "station_key:key-1";

    async fn runtime() -> PersistenceRuntime {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("quality-read.sqlite3"))
            .await
            .expect("initialize runtime");
        std::mem::forget(root);
        runtime
    }

    async fn seed_active_generation(runtime: &PersistenceRuntime) {
        let mut write = runtime.begin_write().await.expect("begin write");
        let hash = "a".repeat(64);
        sqlx::query(
            "INSERT INTO routing_quality_generation_v3 (
                quality_generation_id, scope, quality_policy_revision,
                quality_algorithm_version, status, evaluation_at_ms,
                input_observation_watermark, input_observation_hash,
                output_content_hash, checkpoint_ref, created_at_ms,
                activated_at_ms, updated_at_ms
             ) VALUES (?1, 'all', 1, 'routing_quality_v3', 'active', 1,
                       0, ?2, ?2, 'checkpoint:test', 1, 1, 1)",
        )
        .bind(GENERATION_ID)
        .bind(hash)
        .execute(write.connection())
        .await
        .expect("seed generation");
        write.commit().await.expect("commit generation");
    }

    async fn seed_summary(runtime: &PersistenceRuntime, quality_unavailable: serde_json::Value) {
        let mut write = runtime.begin_write().await.expect("begin write");
        let summary = serde_json::json!({
            "scope": SCOPE,
            "projector_version": "routing_quality_v3",
            "quality_unavailable": quality_unavailable,
        });
        sqlx::query(
            "INSERT INTO routing_quality_summary_v3 (
                quality_generation_id, scope, station_key_id,
                station_key_lifecycle_revision, quality_revision,
                summary_json, updated_at_ms
             ) VALUES (?1, ?2, 'key-1', 1, 7, ?3, 1)",
        )
        .bind(GENERATION_ID)
        .bind(SCOPE)
        .bind(summary.to_string())
        .execute(write.connection())
        .await
        .expect("seed summary");
        for (axis, value) in [("reliability", 9_200_i64), ("latency", 8_500_i64)] {
            sqlx::query(
                "INSERT INTO routing_quality_health_axis_v3 (
                    quality_generation_id, scope, station_key_id,
                    station_key_lifecycle_revision, axis, health_revision,
                    value_basis_points, updated_at_ms
                 ) VALUES (?1, ?2, 'key-1', 1, ?3, 7, ?4, 1)",
            )
            .bind(GENERATION_ID)
            .bind(SCOPE)
            .bind(axis)
            .bind(value)
            .execute(write.connection())
            .await
            .expect("seed axis");
        }
        write.commit().await.expect("commit summary");
    }

    async fn seed_backlog(runtime: &PersistenceRuntime, ingested_at_ms: i64) {
        let mut write = runtime.begin_write().await.expect("begin write");
        sqlx::query(
            "INSERT INTO routing_observations (
                id, producer_id, producer_sequence, payload_hash,
                event_at_ms, ingested_at_ms, scope, source,
                traffic_equivalence, outcome_kind, latency_ms,
                mass_basis_points, evidence_json, created_at_ms
             ) VALUES (
                'quality-backlog', 'quality-test', 1, '0123456789abcdef',
                ?1, ?1, ?2, 'real_request', 'exact_request',
                'success', 100, 10000, '{}', ?1
             )",
        )
        .bind(ingested_at_ms)
        .bind(SCOPE)
        .execute(write.connection())
        .await
        .expect("seed observation");
        write
            .connection()
            .execute(
                "UPDATE routing_observations
                 SET generation_eligibility = 'active'
                 WHERE id = 'quality-backlog'",
            )
            .await
            .expect("mark active backlog");
        write.commit().await.expect("commit backlog");
    }

    async fn load(
        runtime: &PersistenceRuntime,
        scopes: &[String],
        now_ms: i64,
    ) -> super::RoutingQualityPlanningRead {
        let mut read = runtime.begin_read().await.expect("begin read");
        RoutingQualityStore
            .load_planning_read(read.connection(), Some(GENERATION_ID), scopes, now_ms)
            .await
            .expect("load planning quality")
    }

    #[tokio::test]
    async fn planning_read_without_backlog_is_fresh_and_available() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        seed_summary(&runtime, serde_json::Value::Bool(false)).await;

        let read = load(&runtime, &[SCOPE.to_string()], 10_000).await;

        assert!(read.quality_available);
        assert!(!read.quality_stale);
        assert_eq!(read.projection_backlog, 0);
        assert_eq!(read.projection_lag_seconds, 0);
        assert_eq!(read.axes[SCOPE]["reliability"], 9_200);
    }

    #[tokio::test]
    async fn bounded_projection_lag_is_stale_but_still_usable() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        seed_summary(&runtime, serde_json::Value::Bool(false)).await;
        seed_backlog(&runtime, 100_000).await;

        let read = load(&runtime, &[SCOPE.to_string()], 999_000).await;

        assert!(read.quality_available);
        assert!(read.quality_stale);
        assert_eq!(read.projection_backlog, 1);
        assert_eq!(read.projection_lag_seconds, 899);
        assert_eq!(read.axes[SCOPE]["latency"], 8_500);
    }

    #[tokio::test]
    async fn excessive_projection_lag_disables_quality_for_every_candidate() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        seed_summary(&runtime, serde_json::Value::Bool(false)).await;
        seed_backlog(&runtime, 100_000).await;
        let now_ms = 100_000 + (MAX_ACTIVE_QUALITY_LAG_SECONDS as i64 + 1) * 1_000;

        let read = load(&runtime, &[SCOPE.to_string()], now_ms).await;

        assert!(!read.quality_available);
        assert!(read.quality_stale);
        assert_eq!(
            read.projection_lag_seconds,
            MAX_ACTIVE_QUALITY_LAG_SECONDS + 1
        );
    }

    #[tokio::test]
    async fn corrupt_summary_is_unavailable_instead_of_optimistic() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        seed_summary(&runtime, serde_json::Value::String("invalid".into())).await;

        let read = load(&runtime, &[SCOPE.to_string()], 10_000).await;

        assert!(read.unavailable_scopes.contains(SCOPE));
        assert!(!read.axes.contains_key(SCOPE));
    }

    #[tokio::test]
    async fn explicit_quality_unavailable_is_not_used_for_scoring() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        seed_summary(&runtime, serde_json::Value::Bool(true)).await;

        let read = load(&runtime, &[SCOPE.to_string()], 10_000).await;

        assert!(read.unavailable_scopes.contains(SCOPE));
        assert!(!read.axes.contains_key(SCOPE));
    }

    #[tokio::test]
    async fn new_key_without_summary_remains_available_for_optimistic_scoring() {
        let runtime = runtime().await;
        seed_active_generation(&runtime).await;
        let scope = "station_key:new-key".to_string();

        let read = load(&runtime, std::slice::from_ref(&scope), 10_000).await;

        assert!(read.quality_available);
        assert!(!read.unavailable_scopes.contains(&scope));
        assert!(!read.axes.contains_key(&scope));
    }
}
