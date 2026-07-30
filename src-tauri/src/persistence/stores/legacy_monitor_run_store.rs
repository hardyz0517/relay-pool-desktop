use sqlx::Row;

use crate::{
    models::channel_monitors::{ChannelMonitorRun, ChannelMonitorRunCursor, ChannelMonitorRunPage},
    persistence::{error::PersistenceError, read_session::ReadSession},
};

/// Read-only compatibility access for the pre-V2 run history table.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LegacyMonitorRunReader;

impl LegacyMonitorRunReader {
    pub(crate) async fn list_page(
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
                    started_at_ms: run.started_at.parse().unwrap_or_default(),
                    id: run.id.clone(),
                });
        Ok(ChannelMonitorRunPage { items, next_cursor })
    }
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
