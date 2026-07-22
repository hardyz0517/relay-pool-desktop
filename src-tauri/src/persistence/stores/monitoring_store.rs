use sqlx::{Row, SqliteConnection};

use crate::{
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            ChannelMonitorRunCursor, ChannelMonitorRunPage, CreateChannelMonitorInput,
            CreateChannelMonitorRunInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        secrets::redact_text,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringStore;

#[derive(Debug, Clone)]
pub(crate) struct NewMonitorTemplateRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: CreateChannelMonitorTemplateInput,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorTemplatePatch {
    pub(crate) now: String,
    pub(crate) input: UpdateChannelMonitorTemplateInput,
}

#[derive(Debug, Clone)]
pub(crate) struct NewMonitorRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) next_run_at: Option<String>,
    pub(crate) input: CreateChannelMonitorInput,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorPatch {
    pub(crate) now: String,
    pub(crate) input: UpdateChannelMonitorInput,
}

#[derive(Debug, Clone)]
pub(crate) struct NewMonitorRunRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) next_run_at: String,
    pub(crate) input: CreateChannelMonitorRunInput,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelWindowAggregate {
    pub(crate) monitor_id: String,
    pub(crate) total_count: i64,
    pub(crate) success_count: i64,
    pub(crate) failure_count: i64,
    pub(crate) warning_count: i64,
    pub(crate) avg_latency_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelStatusRunRow {
    pub(crate) monitor_id: String,
    pub(crate) run: ChannelMonitorRun,
}

impl MonitoringStore {
    pub(crate) async fn get_template(
        &self,
        read: &mut ReadSession,
        id: &str,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        template_by_id(read.connection(), id).await
    }

    pub(crate) async fn list_templates(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, endpoint_kind, method, path, request_body_json,
                   enabled, built_in, note, created_at, updated_at
            FROM channel_monitor_request_templates INDEXED BY idx_channel_monitor_templates_list
            ORDER BY enabled DESC, built_in DESC, updated_at DESC, id DESC
            LIMIT ?1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_template).collect())
    }

    pub(crate) async fn insert_template(
        &self,
        write: &mut WriteSession,
        row: NewMonitorTemplateRow,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        validate_template(
            &row.input.name,
            &row.input.endpoint_kind,
            &row.input.method,
            &row.input.path,
            &row.input.request_body_json,
        )?;
        sqlx::query(
            r#"
            INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10)
            "#,
        )
        .bind(&row.id)
        .bind(row.input.name.trim())
        .bind(row.input.endpoint_kind.trim())
        .bind(row.input.method.trim().to_uppercase())
        .bind(row.input.path.trim())
        .bind(row.input.request_body_json.trim())
        .bind(bool_to_i64(row.input.enabled))
        .bind(normalize_optional(&row.input.note))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        template_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn update_template(
        &self,
        write: &mut WriteSession,
        patch: MonitorTemplatePatch,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        validate_template(
            &patch.input.name,
            &patch.input.endpoint_kind,
            &patch.input.method,
            &patch.input.path,
            &patch.input.request_body_json,
        )?;
        let changed = sqlx::query(
            r#"
            UPDATE channel_monitor_request_templates
            SET name = ?1,
                endpoint_kind = ?2,
                method = ?3,
                path = ?4,
                request_body_json = ?5,
                enabled = ?6,
                note = ?7,
                updated_at = ?8
            WHERE id = ?9 AND built_in = 0
            "#,
        )
        .bind(patch.input.name.trim())
        .bind(patch.input.endpoint_kind.trim())
        .bind(patch.input.method.trim().to_uppercase())
        .bind(patch.input.path.trim())
        .bind(patch.input.request_body_json.trim())
        .bind(bool_to_i64(patch.input.enabled))
        .bind(normalize_optional(&patch.input.note))
        .bind(&patch.now)
        .bind(&patch.input.id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        template_by_id(write.connection(), &patch.input.id).await
    }

    pub(crate) async fn delete_template(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query(
            r#"
            DELETE FROM channel_monitor_request_templates
            WHERE id = ?1
              AND built_in = 0
              AND NOT EXISTS (SELECT 1 FROM channel_monitors WHERE template_id = ?1)
            "#,
        )
        .bind(id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        Ok(())
    }

    pub(crate) async fn list_monitors(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<ChannelMonitor>, PersistenceError> {
        list_monitors(read.connection(), limit).await
    }

    pub(crate) async fn get_monitor(
        &self,
        read: &mut ReadSession,
        id: &str,
    ) -> Result<ChannelMonitor, PersistenceError> {
        monitor_by_id(read.connection(), id).await
    }

    pub(crate) async fn insert_monitor(
        &self,
        write: &mut WriteSession,
        row: NewMonitorRow,
    ) -> Result<ChannelMonitor, PersistenceError> {
        validate_monitor_input(write.connection(), &row.input).await?;
        let fallback_models = serialize_fallback_models(&row.input.fallback_models)?;
        sqlx::query(
            r#"
            INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                last_run_at, next_run_at, last_status, last_error_message,
                note, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                NULL, ?14, NULL, NULL, ?15, ?16, ?17
            )
            "#,
        )
        .bind(&row.id)
        .bind(row.input.name.trim())
        .bind(row.input.target_type.trim())
        .bind(&row.input.station_id)
        .bind(normalize_optional(&row.input.station_key_id))
        .bind(&row.input.template_id)
        .bind(bool_to_i64(row.input.enabled))
        .bind(row.input.interval_seconds)
        .bind(row.input.jitter_seconds)
        .bind(row.input.timeout_seconds)
        .bind(row.input.max_concurrency)
        .bind(row.input.consecutive_failure_threshold)
        .bind(fallback_models)
        .bind(row.next_run_at)
        .bind(normalize_optional(&row.input.note))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        monitor_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn update_monitor(
        &self,
        write: &mut WriteSession,
        patch: MonitorPatch,
    ) -> Result<ChannelMonitor, PersistenceError> {
        validate_monitor_update(write.connection(), &patch.input).await?;
        let fallback_models = serialize_fallback_models(&patch.input.fallback_models)?;
        let changed = sqlx::query(
            r#"
            UPDATE channel_monitors
            SET name = ?1,
                target_type = ?2,
                station_id = ?3,
                station_key_id = ?4,
                template_id = ?5,
                enabled = ?6,
                interval_seconds = ?7,
                jitter_seconds = ?8,
                timeout_seconds = ?9,
                max_concurrency = ?10,
                consecutive_failure_threshold = ?11,
                fallback_models_json = ?12,
                note = ?13,
                updated_at = ?14
            WHERE id = ?15
            "#,
        )
        .bind(patch.input.name.trim())
        .bind(patch.input.target_type.trim())
        .bind(&patch.input.station_id)
        .bind(normalize_optional(&patch.input.station_key_id))
        .bind(&patch.input.template_id)
        .bind(bool_to_i64(patch.input.enabled))
        .bind(patch.input.interval_seconds)
        .bind(patch.input.jitter_seconds)
        .bind(patch.input.timeout_seconds)
        .bind(patch.input.max_concurrency)
        .bind(patch.input.consecutive_failure_threshold)
        .bind(fallback_models)
        .bind(normalize_optional(&patch.input.note))
        .bind(&patch.now)
        .bind(&patch.input.id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(PersistenceError::NotFound);
        }
        monitor_by_id(write.connection(), &patch.input.id).await
    }

    pub(crate) async fn delete_monitor(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query("DELETE FROM channel_monitors WHERE id = ?1")
            .bind(id)
            .execute(write.connection())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::NotFound);
        }
        Ok(())
    }

    pub(crate) async fn due_monitors(
        &self,
        read: &mut ReadSession,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<ChannelMonitor>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, target_type, station_id, station_key_id, template_id,
                   enabled, interval_seconds, jitter_seconds, timeout_seconds,
                   max_concurrency, consecutive_failure_threshold, fallback_models_json,
                   note, created_at, updated_at
            FROM channel_monitors
            WHERE enabled = 1
              AND (next_run_at IS NULL OR CAST(next_run_at AS INTEGER) <= ?1)
            ORDER BY COALESCE(CAST(next_run_at AS INTEGER), 0) ASC, id ASC
            LIMIT ?2
            "#,
        )
        .bind(now_ms)
        .bind(i64::from(limit))
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_monitor).collect()
    }

    pub(crate) async fn insert_run_and_advance_monitor(
        &self,
        write: &mut WriteSession,
        row: NewMonitorRunRow,
    ) -> Result<ChannelMonitorRun, PersistenceError> {
        validate_run(write.connection(), &row.input).await?;
        let error_message = row
            .input
            .error_message
            .as_deref()
            .map(redact_text)
            .filter(|value| !value.trim().is_empty());
        sqlx::query(
            r#"
            INSERT INTO channel_monitor_runs (
                id, monitor_id, template_id, station_id, station_key_id, status,
                started_at, finished_at, duration_ms, http_status, latency_ms,
                response_model, fallback_model, error_message, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
        )
        .bind(&row.id)
        .bind(&row.input.monitor_id)
        .bind(&row.input.template_id)
        .bind(&row.input.station_id)
        .bind(normalize_optional(&row.input.station_key_id))
        .bind(row.input.status.trim())
        .bind(&row.input.started_at)
        .bind(normalize_optional(&row.input.finished_at))
        .bind(row.input.duration_ms)
        .bind(row.input.http_status)
        .bind(row.input.latency_ms)
        .bind(normalize_optional(&row.input.response_model))
        .bind(normalize_optional(&row.input.fallback_model))
        .bind(&error_message)
        .bind(&row.now)
        .execute(write.connection())
        .await?;

        let last_run_at = row
            .input
            .finished_at
            .as_deref()
            .unwrap_or(&row.input.started_at);
        let changed = sqlx::query(
            r#"
            UPDATE channel_monitors
            SET last_run_at = ?1,
                last_run_id = ?2,
                next_run_at = ?3,
                last_status = ?4,
                last_error_message = ?5,
                updated_at = ?6
            WHERE id = ?7
              AND (
                last_run_at IS NULL
                OR CAST(last_run_at AS INTEGER) < CAST(?1 AS INTEGER)
                OR (
                    CAST(last_run_at AS INTEGER) = CAST(?1 AS INTEGER)
                    AND COALESCE(last_run_id, '') < ?2
                )
              )
            "#,
        )
        .bind(last_run_at)
        .bind(&row.id)
        .bind(&row.next_run_at)
        .bind(row.input.status.trim())
        .bind(&error_message)
        .bind(&row.now)
        .bind(&row.input.monitor_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed > 1 {
            return Err(PersistenceError::InvariantViolation(
                "monitor run advanced more than one monitor".into(),
            ));
        }
        run_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn monitor_schedule(
        &self,
        write: &mut WriteSession,
        monitor_id: &str,
    ) -> Result<(i64, i64), PersistenceError> {
        let row = sqlx::query(
            "SELECT interval_seconds, jitter_seconds FROM channel_monitors WHERE id = ?1",
        )
        .bind(monitor_id)
        .fetch_optional(write.connection())
        .await?
        .ok_or(PersistenceError::NotFound)?;
        Ok((row.get("interval_seconds"), row.get("jitter_seconds")))
    }

    pub(crate) async fn list_run_page(
        &self,
        read: &mut ReadSession,
        monitor_id: &str,
        cursor: Option<&ChannelMonitorRunCursor>,
        limit: u32,
    ) -> Result<ChannelMonitorRunPage, PersistenceError> {
        let fetch_limit = i64::from(limit) + 1;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                r#"
                SELECT id, monitor_id, template_id, station_id, station_key_id,
                       status, started_at, finished_at, duration_ms, http_status,
                       latency_ms, response_model, fallback_model, error_message, created_at
                FROM channel_monitor_runs INDEXED BY idx_channel_monitor_runs_monitor_started
                WHERE monitor_id = ?1
                  AND (
                    CAST(started_at AS INTEGER) < ?2
                    OR (CAST(started_at AS INTEGER) = ?2 AND id < ?3)
                  )
                ORDER BY CAST(started_at AS INTEGER) DESC, id DESC
                LIMIT ?4
                "#,
            )
            .bind(monitor_id)
            .bind(cursor.started_at_ms)
            .bind(&cursor.id)
            .bind(fetch_limit)
            .fetch_all(read.connection())
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, monitor_id, template_id, station_id, station_key_id,
                       status, started_at, finished_at, duration_ms, http_status,
                       latency_ms, response_model, fallback_model, error_message, created_at
                FROM channel_monitor_runs INDEXED BY idx_channel_monitor_runs_monitor_started
                WHERE monitor_id = ?1
                ORDER BY CAST(started_at AS INTEGER) DESC, id DESC
                LIMIT ?2
                "#,
            )
            .bind(monitor_id)
            .bind(fetch_limit)
            .fetch_all(read.connection())
            .await?
        };
        let mut items = rows.into_iter().map(row_to_run).collect::<Vec<_>>();
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor =
            has_more
                .then(|| items.last())
                .flatten()
                .map(|run| ChannelMonitorRunCursor {
                    started_at_ms: parse_millis(&run.started_at).unwrap_or_default(),
                    id: run.id.clone(),
                });
        Ok(ChannelMonitorRunPage { items, next_cursor })
    }

    pub(crate) async fn recent_status_runs(
        &self,
        read: &mut ReadSession,
        monitor_limit: u32,
        run_limit: u32,
    ) -> Result<Vec<ChannelStatusRunRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            WITH bounded_monitors AS (
                SELECT id
                FROM channel_monitors INDEXED BY idx_channel_monitors_list
                ORDER BY enabled DESC, created_at ASC, id ASC
                LIMIT ?1
            )
            SELECT r.id, r.monitor_id, r.template_id, r.station_id, r.station_key_id,
                   r.status, r.started_at, r.finished_at, r.duration_ms, r.http_status,
                   r.latency_ms, r.response_model, r.fallback_model, r.error_message,
                   r.created_at
            FROM bounded_monitors m
            JOIN channel_monitor_runs r ON r.id IN (
                SELECT recent.id
                FROM channel_monitor_runs recent INDEXED BY idx_channel_monitor_runs_monitor_started
                WHERE recent.monitor_id = m.id
                ORDER BY CAST(recent.started_at AS INTEGER) DESC, recent.id DESC
                LIMIT ?2
            )
            ORDER BY r.monitor_id ASC, CAST(r.started_at AS INTEGER) DESC, r.id DESC
            "#,
        )
        .bind(i64::from(monitor_limit))
        .bind(i64::from(run_limit))
        .fetch_all(read.connection())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ChannelStatusRunRow {
                monitor_id: row.get("monitor_id"),
                run: row_to_run(row),
            })
            .collect())
    }

    pub(crate) async fn summary_runs(
        &self,
        read: &mut ReadSession,
        run_since_ms: Option<i64>,
        monitor_limit: u32,
        run_limit: u32,
    ) -> Result<Vec<ChannelStatusRunRow>, PersistenceError> {
        let rows = if let Some(run_since_ms) = run_since_ms {
            sqlx::query(
                r#"
                WITH bounded_monitors AS (
                    SELECT id
                    FROM channel_monitors INDEXED BY idx_channel_monitors_list
                    ORDER BY enabled DESC, created_at ASC, id ASC
                    LIMIT ?1
                )
                SELECT r.id, r.monitor_id, r.template_id, r.station_id, r.station_key_id,
                       r.status, r.started_at, r.finished_at, r.duration_ms, r.http_status,
                       r.latency_ms, r.response_model, r.fallback_model, r.error_message,
                       r.created_at
                FROM bounded_monitors m
                JOIN channel_monitor_runs r ON r.id IN (
                    SELECT recent.id
                    FROM channel_monitor_runs recent INDEXED BY idx_channel_monitor_runs_monitor_started
                    WHERE recent.monitor_id = m.id
                      AND CAST(recent.started_at AS INTEGER) >= ?3
                    ORDER BY CAST(recent.started_at AS INTEGER) DESC, recent.id DESC
                    LIMIT ?2
                )
                ORDER BY r.monitor_id ASC, CAST(r.started_at AS INTEGER) DESC, r.id DESC
                "#,
            )
            .bind(i64::from(monitor_limit))
            .bind(i64::from(run_limit))
            .bind(run_since_ms)
            .fetch_all(read.connection())
            .await?
        } else {
            sqlx::query(
                r#"
                WITH bounded_monitors AS (
                    SELECT id
                    FROM channel_monitors INDEXED BY idx_channel_monitors_list
                    ORDER BY enabled DESC, created_at ASC, id ASC
                    LIMIT ?1
                )
                SELECT r.id, r.monitor_id, r.template_id, r.station_id, r.station_key_id,
                       r.status, r.started_at, r.finished_at, r.duration_ms, r.http_status,
                       r.latency_ms, r.response_model, r.fallback_model, r.error_message,
                       r.created_at
                FROM bounded_monitors m
                JOIN channel_monitor_runs r ON r.id IN (
                    SELECT recent.id
                    FROM channel_monitor_runs recent INDEXED BY idx_channel_monitor_runs_monitor_started
                    WHERE recent.monitor_id = m.id
                    ORDER BY CAST(recent.started_at AS INTEGER) DESC, recent.id DESC
                    LIMIT ?2
                )
                ORDER BY r.monitor_id ASC, CAST(r.started_at AS INTEGER) DESC, r.id DESC
                "#,
            )
            .bind(i64::from(monitor_limit))
            .bind(i64::from(run_limit))
            .fetch_all(read.connection())
            .await?
        };
        Ok(rows
            .into_iter()
            .map(|row| ChannelStatusRunRow {
                monitor_id: row.get("monitor_id"),
                run: row_to_run(row),
            })
            .collect())
    }

    pub(crate) async fn window_aggregates(
        &self,
        read: &mut ReadSession,
        since_ms: i64,
        monitor_limit: u32,
    ) -> Result<Vec<ChannelWindowAggregate>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            WITH bounded_monitors AS (
                SELECT id
                FROM channel_monitors INDEXED BY idx_channel_monitors_list
                ORDER BY enabled DESC, created_at ASC, id ASC
                LIMIT ?1
            )
            SELECT m.id AS monitor_id,
                   COUNT(r.id) AS total_count,
                   COALESCE(SUM(CASE WHEN r.status = 'success' THEN 1 ELSE 0 END), 0) AS success_count,
                   COALESCE(SUM(CASE WHEN r.status = 'failed' THEN 1 ELSE 0 END), 0) AS failure_count,
                   COALESCE(SUM(CASE WHEN r.status IN ('warning', 'skipped') THEN 1 ELSE 0 END), 0) AS warning_count,
                   CAST(AVG(COALESCE(r.latency_ms, r.duration_ms)) AS INTEGER) AS avg_latency_ms
            FROM bounded_monitors m
            LEFT JOIN channel_monitor_runs r
              ON r.monitor_id = m.id
             AND CAST(r.started_at AS INTEGER) >= ?2
            GROUP BY m.id
            ORDER BY m.id ASC
            "#,
        )
        .bind(i64::from(monitor_limit))
        .bind(since_ms)
        .fetch_all(read.connection())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ChannelWindowAggregate {
                monitor_id: row.get("monitor_id"),
                total_count: row.get("total_count"),
                success_count: row.get("success_count"),
                failure_count: row.get("failure_count"),
                warning_count: row.get("warning_count"),
                avg_latency_ms: row.get("avg_latency_ms"),
            })
            .collect())
    }
}

async fn list_monitors(
    connection: &mut SqliteConnection,
    limit: u32,
) -> Result<Vec<ChannelMonitor>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, target_type, station_id, station_key_id, template_id,
               enabled, interval_seconds, jitter_seconds, timeout_seconds,
               max_concurrency, consecutive_failure_threshold, fallback_models_json,
               note, created_at, updated_at
        FROM channel_monitors INDEXED BY idx_channel_monitors_list
        ORDER BY enabled DESC, created_at ASC, id ASC
        LIMIT ?1
        "#,
    )
    .bind(i64::from(limit))
    .fetch_all(connection)
    .await?;
    rows.into_iter().map(row_to_monitor).collect()
}

async fn template_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, endpoint_kind, method, path, request_body_json,
               enabled, built_in, note, created_at, updated_at
        FROM channel_monitor_request_templates WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_template).ok_or(PersistenceError::NotFound)
}

async fn monitor_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ChannelMonitor, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, target_type, station_id, station_key_id, template_id,
               enabled, interval_seconds, jitter_seconds, timeout_seconds,
               max_concurrency, consecutive_failure_threshold, fallback_models_json,
               note, created_at, updated_at
        FROM channel_monitors WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_monitor)
        .transpose()?
        .ok_or(PersistenceError::NotFound)
}

async fn run_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ChannelMonitorRun, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, monitor_id, template_id, station_id, station_key_id,
               status, started_at, finished_at, duration_ms, http_status,
               latency_ms, response_model, fallback_model, error_message, created_at
        FROM channel_monitor_runs WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_run).ok_or(PersistenceError::NotFound)
}

async fn validate_monitor_input(
    connection: &mut SqliteConnection,
    input: &CreateChannelMonitorInput,
) -> Result<(), PersistenceError> {
    validate_monitor_values(
        &input.name,
        &input.target_type,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    validate_monitor_owners(
        connection,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        &input.target_type,
    )
    .await
}

async fn validate_monitor_update(
    connection: &mut SqliteConnection,
    input: &UpdateChannelMonitorInput,
) -> Result<(), PersistenceError> {
    validate_monitor_values(
        &input.name,
        &input.target_type,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    validate_monitor_owners(
        connection,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        &input.target_type,
    )
    .await
}

async fn validate_monitor_owners(
    connection: &mut SqliteConnection,
    station_id: &str,
    station_key_id: Option<&str>,
    template_id: &str,
    target_type: &str,
) -> Result<(), PersistenceError> {
    let station_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_one(&mut *connection)
            .await?
            == 1;
    let template_enabled = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channel_monitor_request_templates WHERE id = ?1 AND enabled = 1",
    )
    .bind(template_id)
    .fetch_one(&mut *connection)
    .await?
        == 1;
    if !station_exists || !template_enabled {
        return Err(PersistenceError::ConstraintViolation);
    }
    match (target_type.trim(), station_key_id) {
        ("station", None) => Ok(()),
        ("station_key", Some(station_key_id)) => {
            let owned = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
            )
            .bind(station_key_id)
            .bind(station_id)
            .fetch_one(connection)
            .await?
                == 1;
            if owned {
                Ok(())
            } else {
                Err(PersistenceError::ConstraintViolation)
            }
        }
        _ => Err(PersistenceError::ConstraintViolation),
    }
}

async fn validate_run(
    connection: &mut SqliteConnection,
    input: &CreateChannelMonitorRunInput,
) -> Result<(), PersistenceError> {
    let started_at = parse_millis(&input.started_at);
    let finished_at = input.finished_at.as_deref().and_then(parse_millis);
    if !matches!(
        input.status.trim(),
        "success" | "warning" | "failed" | "skipped"
    ) || started_at.is_none()
        || input
            .finished_at
            .as_deref()
            .is_some_and(|value| parse_millis(value).is_none())
        || finished_at
            .zip(started_at)
            .is_some_and(|(finished, started)| finished < started)
        || input.duration_ms.is_some_and(|value| value < 0)
        || input
            .http_status
            .is_some_and(|value| !(100..=599).contains(&value))
        || input.latency_ms.is_some_and(|value| value < 0)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    let monitor = sqlx::query(
        "SELECT station_id, station_key_id, template_id, target_type FROM channel_monitors WHERE id = ?1",
    )
    .bind(&input.monitor_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(PersistenceError::NotFound)?;
    let station_id: String = monitor.get("station_id");
    let station_key_id: Option<String> = monitor.get("station_key_id");
    let template_id: String = monitor.get("template_id");
    let target_type: String = monitor.get("target_type");
    if input.station_id != station_id || input.template_id != template_id {
        return Err(PersistenceError::ConstraintViolation);
    }
    match target_type.as_str() {
        "station_key" if input.station_key_id == station_key_id => Ok(()),
        "station" => {
            if let Some(run_key_id) = input.station_key_id.as_deref() {
                let owned = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
                )
                .bind(run_key_id)
                .bind(&input.station_id)
                .fetch_one(connection)
                .await?
                    == 1;
                if !owned {
                    return Err(PersistenceError::ConstraintViolation);
                }
            }
            Ok(())
        }
        _ => Err(PersistenceError::ConstraintViolation),
    }
}

fn validate_template(
    name: &str,
    endpoint_kind: &str,
    method: &str,
    path: &str,
    request_body_json: &str,
) -> Result<(), PersistenceError> {
    let body = serde_json::from_str::<serde_json::Value>(request_body_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if name.trim().is_empty()
        || endpoint_kind.trim().is_empty()
        || method.trim().is_empty()
        || path.trim().is_empty()
        || !body.is_object()
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_monitor_values(
    name: &str,
    target_type: &str,
    interval_seconds: i64,
    jitter_seconds: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    failure_threshold: i64,
) -> Result<(), PersistenceError> {
    if name.trim().is_empty()
        || !matches!(target_type.trim(), "station" | "station_key")
        || !(15..=3600).contains(&interval_seconds)
        || !(0..=600).contains(&jitter_seconds)
        || interval_seconds - jitter_seconds < 15
        || !(5..=120).contains(&timeout_seconds)
        || !(1..=16).contains(&max_concurrency)
        || !(1..=20).contains(&failure_threshold)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn row_to_template(row: sqlx::sqlite::SqliteRow) -> ChannelMonitorRequestTemplate {
    ChannelMonitorRequestTemplate {
        id: row.get("id"),
        name: row.get("name"),
        endpoint_kind: row.get("endpoint_kind"),
        method: row.get("method"),
        path: row.get("path"),
        request_body_json: row.get("request_body_json"),
        enabled: i64_to_bool(row.get("enabled")),
        built_in: i64_to_bool(row.get("built_in")),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_monitor(row: sqlx::sqlite::SqliteRow) -> Result<ChannelMonitor, PersistenceError> {
    let fallback_models_json: String = row.get("fallback_models_json");
    let fallback_models = serde_json::from_str(&fallback_models_json).map_err(|_| {
        PersistenceError::InvariantViolation("invalid monitor fallback models".into())
    })?;
    Ok(ChannelMonitor {
        id: row.get("id"),
        name: row.get("name"),
        target_type: row.get("target_type"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        template_id: row.get("template_id"),
        enabled: i64_to_bool(row.get("enabled")),
        interval_seconds: row.get("interval_seconds"),
        jitter_seconds: row.get("jitter_seconds"),
        timeout_seconds: row.get("timeout_seconds"),
        max_concurrency: row.get("max_concurrency"),
        consecutive_failure_threshold: row.get("consecutive_failure_threshold"),
        fallback_models,
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_run(row: sqlx::sqlite::SqliteRow) -> ChannelMonitorRun {
    ChannelMonitorRun {
        id: row.get("id"),
        monitor_id: row.get("monitor_id"),
        template_id: row.get("template_id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        status: row.get("status"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        duration_ms: row.get("duration_ms"),
        http_status: row.get("http_status"),
        latency_ms: row.get("latency_ms"),
        response_model: row.get("response_model"),
        fallback_model: row.get("fallback_model"),
        error_message: row.get("error_message"),
        created_at: row.get("created_at"),
    }
}

fn serialize_fallback_models(values: &[String]) -> Result<String, PersistenceError> {
    let normalized = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    serde_json::to_string(&normalized).map_err(|_| PersistenceError::ConstraintViolation)
}

fn parse_millis(value: &str) -> Option<i64> {
    value.trim().parse().ok()
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn bool_to_i64(value: bool) -> i64 {
    i64::from(value)
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}
