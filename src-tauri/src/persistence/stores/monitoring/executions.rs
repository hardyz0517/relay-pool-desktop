use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone)]
pub(crate) struct NewExecutionRow {
    pub(crate) id: String,
    pub(crate) monitor_id: String,
    pub(crate) trigger_kind: String,
    pub(crate) trigger_request_id: Option<String>,
    pub(crate) status: String,
    pub(crate) planned_at_ms: i64,
    pub(crate) started_at_ms: Option<i64>,
    pub(crate) config_revision: i64,
    pub(crate) config_snapshot_hash: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) target_count: i64,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct NewAttemptRow {
    pub(crate) id: String,
    pub(crate) execution_id: String,
    pub(crate) monitor_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) model: String,
    pub(crate) model_role: String,
    pub(crate) model_index: i64,
    pub(crate) attempt_number: i64,
    pub(crate) protocol_kind: String,
    pub(crate) client_profile_id: String,
    pub(crate) client_profile_version: i64,
    pub(crate) request_profile_hash: String,
    pub(crate) transport_mode: String,
    pub(crate) started_at_ms: i64,
    pub(crate) finished_at_ms: Option<i64>,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) http_status: Option<i64>,
    pub(crate) outcome: String,
    pub(crate) failure_kind: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) response_model: Option<String>,
    pub(crate) content_extracted: bool,
    pub(crate) validation_passed: bool,
    pub(crate) output_bytes: i64,
    pub(crate) error_summary: Option<String>,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct FinalizeTargetRow {
    pub(crate) id: String,
    pub(crate) execution_id: String,
    pub(crate) monitor_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) terminal_outcome: String,
    pub(crate) terminal_failure_kind: Option<String>,
    pub(crate) requested_model: String,
    pub(crate) effective_model: Option<String>,
    pub(crate) used_fallback: bool,
    pub(crate) attempt_count: i64,
    pub(crate) decisive_attempt_id: Option<String>,
    pub(crate) protocol_kind: Option<String>,
    pub(crate) resolved_adapter_kind: String,
    pub(crate) client_profile_id: String,
    pub(crate) client_profile_version: i64,
    pub(crate) request_profile_hash: Option<String>,
    pub(crate) traffic_equivalence: String,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) semantic_confidence: String,
    pub(crate) started_at_ms: i64,
    pub(crate) finished_at_ms: Option<i64>,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionSummaryRow {
    pub(crate) status: String,
    pub(crate) available_count: i64,
    pub(crate) degraded_count: i64,
    pub(crate) unavailable_count: i64,
    pub(crate) skipped_count: i64,
    pub(crate) summary_outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CancelExecutionRow {
    pub(crate) execution_id: String,
    pub(crate) status: String,
    pub(crate) cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TriggeredExecutionRow {
    pub(crate) id: String,
    pub(crate) monitor_id: String,
    pub(crate) status: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringExecutionRepository;

impl MonitoringExecutionRepository {
    pub(crate) async fn insert_execution(
        &self,
        connection: &mut SqliteConnection,
        row: &NewExecutionRow,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO channel_monitor_executions (
                id, monitor_id, trigger_kind, trigger_request_id, status, planned_at_ms,
                started_at_ms, config_revision, config_snapshot_hash, endpoint_revision,
                target_count, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(&row.id)
        .bind(&row.monitor_id)
        .bind(&row.trigger_kind)
        .bind(&row.trigger_request_id)
        .bind(&row.status)
        .bind(row.planned_at_ms)
        .bind(row.started_at_ms)
        .bind(row.config_revision)
        .bind(&row.config_snapshot_hash)
        .bind(row.endpoint_revision)
        .bind(row.target_count)
        .bind(row.created_at_ms)
        .execute(connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn cancel_execution(
        &self,
        connection: &mut SqliteConnection,
        execution_id: &str,
        now_ms: i64,
    ) -> Result<CancelExecutionRow, PersistenceError> {
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM channel_monitor_executions WHERE id = ?1")
                .bind(execution_id)
                .fetch_one(&mut *connection)
                .await?;
        let cancelled = matches!(current_status.as_str(), "queued" | "running");
        if cancelled {
            sqlx::query(
                r#"
                UPDATE channel_monitor_executions
                SET status = 'cancelled',
                    finished_at_ms = COALESCE(finished_at_ms, ?1),
                    summary_failure_kind = COALESCE(summary_failure_kind, 'cancelled')
                WHERE id = ?2
                "#,
            )
            .bind(now_ms)
            .bind(execution_id)
            .execute(&mut *connection)
            .await?;
        }
        Ok(CancelExecutionRow {
            execution_id: execution_id.to_string(),
            status: if cancelled {
                "cancelled".to_string()
            } else {
                current_status
            },
            cancelled,
        })
    }

    pub(crate) async fn find_by_trigger_request_id(
        &self,
        connection: &mut SqliteConnection,
        trigger_request_id: &str,
    ) -> Result<Option<TriggeredExecutionRow>, PersistenceError> {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, monitor_id, status
            FROM channel_monitor_executions
            WHERE trigger_request_id = ?1
            ORDER BY created_at_ms DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(trigger_request_id)
        .fetch_optional(connection)
        .await
        .map(|row| {
            row.map(|(id, monitor_id, status)| TriggeredExecutionRow {
                id,
                monitor_id,
                status,
            })
        })
        .map_err(PersistenceError::from)
    }

    pub(crate) async fn start_queued_execution(
        &self,
        connection: &mut SqliteConnection,
        execution_id: &str,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        sqlx::query(
            r#"
            UPDATE channel_monitor_executions
            SET status = 'running', started_at_ms = COALESCE(started_at_ms, ?1)
            WHERE id = ?2 AND status = 'queued'
            "#,
        )
        .bind(now_ms)
        .bind(execution_id)
        .execute(connection)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(PersistenceError::from)
    }

    pub(crate) async fn interrupt_execution(
        &self,
        connection: &mut SqliteConnection,
        execution_id: &str,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        sqlx::query(
            r#"
            UPDATE channel_monitor_executions
            SET status = 'interrupted', finished_at_ms = COALESCE(finished_at_ms, ?1),
                summary_failure_kind = COALESCE(summary_failure_kind, 'interrupted')
            WHERE id = ?2 AND status IN ('queued', 'running')
            "#,
        )
        .bind(now_ms)
        .bind(execution_id)
        .execute(connection)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(PersistenceError::from)
    }

    pub(crate) async fn append_attempt(
        &self,
        connection: &mut SqliteConnection,
        row: &NewAttemptRow,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO channel_monitor_attempts (
                id, execution_id, monitor_id, station_id, station_key_id, model,
                model_role, model_index, attempt_number, protocol_kind,
                client_profile_id, client_profile_version, request_profile_hash,
                transport_mode, started_at_ms, finished_at_ms, latency_ms, http_status,
                outcome, failure_kind, retryable, response_model, content_extracted,
                validation_passed, output_bytes, error_summary, created_at_ms
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                ?25, ?26, ?27
            )
            "#,
        )
        .bind(&row.id)
        .bind(&row.execution_id)
        .bind(&row.monitor_id)
        .bind(&row.station_id)
        .bind(&row.station_key_id)
        .bind(&row.model)
        .bind(&row.model_role)
        .bind(row.model_index)
        .bind(row.attempt_number)
        .bind(&row.protocol_kind)
        .bind(&row.client_profile_id)
        .bind(row.client_profile_version)
        .bind(&row.request_profile_hash)
        .bind(&row.transport_mode)
        .bind(row.started_at_ms)
        .bind(row.finished_at_ms)
        .bind(row.latency_ms)
        .bind(row.http_status)
        .bind(&row.outcome)
        .bind(&row.failure_kind)
        .bind(i64::from(row.retryable))
        .bind(&row.response_model)
        .bind(i64::from(row.content_extracted))
        .bind(i64::from(row.validation_passed))
        .bind(row.output_bytes)
        .bind(&row.error_summary)
        .bind(row.created_at_ms)
        .execute(connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn finalize_target(
        &self,
        connection: &mut SqliteConnection,
        row: &FinalizeTargetRow,
    ) -> Result<(), PersistenceError> {
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_monitor_target_results WHERE id = ?1",
        )
        .bind(&row.id)
        .fetch_one(&mut *connection)
        .await?;
        if existing > 0 {
            return Ok(());
        }

        let decisive = sqlx::query(
            r#"
            SELECT COUNT(*) AS attempt_count
            FROM channel_monitor_attempts
            WHERE execution_id = ?1
              AND station_key_id = ?2
        "#,
        )
        .bind(&row.execution_id)
        .bind(&row.station_key_id)
        .fetch_one(&mut *connection)
        .await?;
        if decisive.get::<i64, _>("attempt_count") != row.attempt_count {
            return Err(PersistenceError::ConstraintViolation);
        }

        match row.decisive_attempt_id.as_deref() {
            Some(decisive_attempt_id) => {
                let decisive_owner = sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)
                    FROM channel_monitor_attempts
                    WHERE id = ?1
                      AND execution_id = ?2
                      AND station_key_id = ?3
                    "#,
                )
                .bind(decisive_attempt_id)
                .bind(&row.execution_id)
                .bind(&row.station_key_id)
                .fetch_one(&mut *connection)
                .await?;
                if decisive_owner != 1 {
                    return Err(PersistenceError::ConstraintViolation);
                }
            }
            None if row.terminal_outcome == "skipped" && row.attempt_count == 0 => {}
            None => return Err(PersistenceError::ConstraintViolation),
        }

        sqlx::query(
            r#"
            INSERT INTO channel_monitor_target_results (
                id, execution_id, monitor_id, station_id, station_key_id,
                terminal_outcome, terminal_failure_kind, requested_model,
                effective_model, used_fallback, attempt_count, decisive_attempt_id,
                protocol_kind, resolved_adapter_kind, client_profile_id,
                client_profile_version, request_profile_hash, traffic_equivalence,
                latency_ms,
                semantic_confidence, started_at_ms, finished_at_ms, created_at_ms
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
            )
            "#,
        )
        .bind(&row.id)
        .bind(&row.execution_id)
        .bind(&row.monitor_id)
        .bind(&row.station_id)
        .bind(&row.station_key_id)
        .bind(&row.terminal_outcome)
        .bind(&row.terminal_failure_kind)
        .bind(&row.requested_model)
        .bind(&row.effective_model)
        .bind(i64::from(row.used_fallback))
        .bind(row.attempt_count)
        .bind(&row.decisive_attempt_id)
        .bind(&row.protocol_kind)
        .bind(&row.resolved_adapter_kind)
        .bind(&row.client_profile_id)
        .bind(row.client_profile_version)
        .bind(&row.request_profile_hash)
        .bind(&row.traffic_equivalence)
        .bind(row.latency_ms)
        .bind(&row.semantic_confidence)
        .bind(row.started_at_ms)
        .bind(row.finished_at_ms)
        .bind(row.created_at_ms)
        .execute(&mut *connection)
        .await?;

        Ok(())
    }

    pub(crate) async fn finalize_execution_and_advance_schedule(
        &self,
        connection: &mut SqliteConnection,
        execution_id: &str,
        monitor_id: &str,
        finished_at_ms: i64,
        next_due_at_ms: Option<i64>,
    ) -> Result<ExecutionSummaryRow, PersistenceError> {
        let execution = sqlx::query(
            "SELECT target_count FROM channel_monitor_executions WHERE id = ?1 AND monitor_id = ?2",
        )
        .bind(execution_id)
        .bind(monitor_id)
        .fetch_one(&mut *connection)
        .await?;
        let expected_targets = execution.get::<i64, _>("target_count");
        let actual_targets = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_monitor_target_results WHERE execution_id = ?1",
        )
        .bind(execution_id)
        .fetch_one(&mut *connection)
        .await?;
        if actual_targets != expected_targets {
            return Err(PersistenceError::ConstraintViolation);
        }

        let summary = sqlx::query(
            r#"
            SELECT
                SUM(CASE WHEN terminal_outcome = 'available' THEN 1 ELSE 0 END) AS available_count,
                SUM(CASE WHEN terminal_outcome = 'degraded' THEN 1 ELSE 0 END) AS degraded_count,
                SUM(CASE WHEN terminal_outcome = 'unavailable' THEN 1 ELSE 0 END) AS unavailable_count,
                SUM(CASE WHEN terminal_outcome = 'skipped' THEN 1 ELSE 0 END) AS skipped_count
            FROM channel_monitor_target_results
            WHERE execution_id = ?1
            "#,
        )
        .bind(execution_id)
        .fetch_one(&mut *connection)
        .await?;
        let available_count = summary.get::<i64, _>("available_count");
        let degraded_count = summary.get::<i64, _>("degraded_count");
        let unavailable_count = summary.get::<i64, _>("unavailable_count");
        let skipped_count = summary.get::<i64, _>("skipped_count");
        let summary_outcome = execution_summary_outcome(
            available_count,
            degraded_count,
            unavailable_count,
            skipped_count,
        );

        sqlx::query(
            r#"
            UPDATE channel_monitor_executions
            SET status = 'completed',
                finished_at_ms = ?1,
                available_count = ?2,
                degraded_count = ?3,
                unavailable_count = ?4,
                skipped_count = ?5,
                summary_outcome = ?6
            WHERE id = ?7
            "#,
        )
        .bind(finished_at_ms)
        .bind(available_count)
        .bind(degraded_count)
        .bind(unavailable_count)
        .bind(skipped_count)
        .bind(&summary_outcome)
        .bind(execution_id)
        .execute(&mut *connection)
        .await?;

        if let Some(next_due_at_ms) = next_due_at_ms {
            sqlx::query("UPDATE channel_monitors SET next_due_at_ms = ?1 WHERE id = ?2")
                .bind(next_due_at_ms)
                .bind(monitor_id)
                .execute(&mut *connection)
                .await?;
        }

        Ok(ExecutionSummaryRow {
            status: "completed".to_string(),
            available_count,
            degraded_count,
            unavailable_count,
            skipped_count,
            summary_outcome,
        })
    }

    pub(crate) async fn mark_startup_recovery_interrupted(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
    ) -> Result<i64, PersistenceError> {
        let affected_monitors = sqlx::query(
            r#"
            SELECT DISTINCT e.monitor_id, m.interval_seconds
            FROM channel_monitor_executions e
            JOIN channel_monitors m ON m.id = e.monitor_id
            WHERE e.status IN ('queued', 'running')
            "#,
        )
        .fetch_all(&mut *connection)
        .await?;

        let affected_count = sqlx::query(
            r#"
            UPDATE channel_monitor_executions
            SET status = 'interrupted',
                finished_at_ms = ?1,
                summary_failure_kind = 'interrupted'
            WHERE status IN ('queued', 'running')
            "#,
        )
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();

        for row in affected_monitors {
            let monitor_id = row.get::<String, _>("monitor_id");
            let interval_seconds = row.get::<i64, _>("interval_seconds").max(1);
            let next_due_at_ms = now_ms.saturating_add(interval_seconds.saturating_mul(1_000));
            sqlx::query("UPDATE channel_monitors SET next_due_at_ms = ?1 WHERE id = ?2")
                .bind(next_due_at_ms)
                .bind(monitor_id)
                .execute(&mut *connection)
                .await?;
        }

        i64::try_from(affected_count).map_err(|_| PersistenceError::ConstraintViolation)
    }
}

fn execution_summary_outcome(
    available_count: i64,
    degraded_count: i64,
    unavailable_count: i64,
    skipped_count: i64,
) -> Option<String> {
    if available_count > 0 && degraded_count == 0 && unavailable_count == 0 {
        Some("available".to_string())
    } else if available_count > 0 || degraded_count > 0 {
        Some("degraded".to_string())
    } else if unavailable_count > 0 {
        Some("unavailable".to_string())
    } else if skipped_count > 0 {
        Some("skipped".to_string())
    } else {
        None
    }
}
