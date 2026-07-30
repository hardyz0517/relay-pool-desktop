use std::collections::BTreeMap;

use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

const RECENT_TARGET_RESULT_LIMIT: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentTargetResultRow {
    pub(crate) id: String,
    pub(crate) execution_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) terminal_outcome: String,
    pub(crate) finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusBucketRollupRow {
    pub(crate) monitor_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) bucket_kind: String,
    pub(crate) bucket_start_ms: i64,
    pub(crate) bucket_end_ms: i64,
    pub(crate) total_count: i64,
    pub(crate) available_count: i64,
    pub(crate) degraded_count: i64,
    pub(crate) unavailable_count: i64,
    pub(crate) skipped_count: i64,
    pub(crate) failure_counts: BTreeMap<String, u32>,
    pub(crate) corrupt_failure_counts: bool,
    pub(crate) dirty: bool,
    pub(crate) p50_latency_ms: Option<i64>,
    pub(crate) p95_latency_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringStatusQueryRepository;

impl MonitoringStatusQueryRepository {
    pub(crate) async fn recent_target_results(
        &self,
        connection: &mut SqliteConnection,
        monitor_id: &str,
        cursor: Option<(i64, String)>,
        limit: u32,
    ) -> Result<Vec<RecentTargetResultRow>, PersistenceError> {
        let bounded_limit = i64::from(limit.clamp(1, RECENT_TARGET_RESULT_LIMIT));
        let rows = if let Some((finished_at_ms, id)) = cursor {
            sqlx::query(
                r#"
                SELECT id, execution_id, station_key_id, terminal_outcome, finished_at_ms
                FROM channel_monitor_target_results
                WHERE monitor_id = ?1
                  AND (
                    finished_at_ms < ?2
                    OR (finished_at_ms = ?2 AND id < ?3)
                  )
                ORDER BY finished_at_ms DESC, id DESC
                LIMIT ?4
                "#,
            )
            .bind(monitor_id)
            .bind(finished_at_ms)
            .bind(id)
            .bind(bounded_limit)
            .fetch_all(&mut *connection)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, execution_id, station_key_id, terminal_outcome, finished_at_ms
                FROM channel_monitor_target_results
                WHERE monitor_id = ?1
                ORDER BY finished_at_ms DESC, id DESC
                LIMIT ?2
                "#,
            )
            .bind(monitor_id)
            .bind(bounded_limit)
            .fetch_all(&mut *connection)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(|row| RecentTargetResultRow {
                id: row.get("id"),
                execution_id: row.get("execution_id"),
                station_key_id: row.get("station_key_id"),
                terminal_outcome: row.get("terminal_outcome"),
                finished_at_ms: row.get("finished_at_ms"),
            })
            .collect())
    }

    pub(crate) async fn bucket_rollups(
        &self,
        connection: &mut SqliteConnection,
        monitor_id: &str,
        station_key_id: Option<&str>,
        bucket_kind: &str,
        range_start_ms: i64,
        range_end_ms: i64,
    ) -> Result<Vec<StatusBucketRollupRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT
                br.monitor_id,
                br.station_key_id,
                br.bucket_kind,
                br.bucket_start_ms,
                br.bucket_end_ms,
                br.total_count,
                br.available_count,
                br.degraded_count,
                br.unavailable_count,
                br.skipped_count,
                br.failure_counts_json,
                br.p50_latency_ms,
                br.p95_latency_ms,
                EXISTS (
                    SELECT 1
                    FROM channel_monitor_rollup_dirty_ranges dr
                    WHERE dr.monitor_id = br.monitor_id
                      AND (dr.station_key_id IS br.station_key_id OR dr.station_key_id = br.station_key_id)
                      AND dr.range_start_ms < br.bucket_end_ms
                      AND dr.range_end_ms > br.bucket_start_ms
                ) AS dirty
            FROM channel_monitor_bucket_rollups br
            WHERE br.monitor_id = ?1
              AND (br.station_key_id IS ?2 OR br.station_key_id = ?2)
              AND br.bucket_kind = ?3
              AND br.bucket_start_ms >= ?4
              AND br.bucket_start_ms < ?5
            ORDER BY br.bucket_start_ms ASC
            "#,
        )
        .bind(monitor_id)
        .bind(station_key_id)
        .bind(bucket_kind)
        .bind(range_start_ms)
        .bind(range_end_ms)
        .fetch_all(connection)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let failure_counts_json: String = row.get("failure_counts_json");
                let parsed_failure_counts =
                    serde_json::from_str::<BTreeMap<String, u32>>(&failure_counts_json);
                let corrupt_failure_counts = parsed_failure_counts.is_err();
                StatusBucketRollupRow {
                    monitor_id: row.get("monitor_id"),
                    station_key_id: row.get("station_key_id"),
                    bucket_kind: row.get("bucket_kind"),
                    bucket_start_ms: row.get("bucket_start_ms"),
                    bucket_end_ms: row.get("bucket_end_ms"),
                    total_count: row.get("total_count"),
                    available_count: row.get("available_count"),
                    degraded_count: row.get("degraded_count"),
                    unavailable_count: row.get("unavailable_count"),
                    skipped_count: row.get("skipped_count"),
                    failure_counts: parsed_failure_counts.unwrap_or_default(),
                    corrupt_failure_counts,
                    dirty: row.get::<i64, _>("dirty") != 0,
                    p50_latency_ms: row.get("p50_latency_ms"),
                    p95_latency_ms: row.get("p95_latency_ms"),
                }
            })
            .collect())
    }
}
