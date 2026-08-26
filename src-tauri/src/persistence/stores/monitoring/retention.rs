use std::collections::BTreeMap;

use serde_json::json;
use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RollupDirtyRangeRow {
    pub(crate) id: String,
    pub(crate) monitor_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) range_start_ms: i64,
    pub(crate) range_end_ms: i64,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RollupRepairOutcome {
    pub(crate) repaired_ranges: u32,
    pub(crate) written_rollups: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetentionDeleteOutcome {
    pub(crate) deleted_executions: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringRetentionRepository;

impl MonitoringRetentionRepository {
    pub(crate) async fn mark_dirty_range(
        &self,
        connection: &mut SqliteConnection,
        id: &str,
        monitor_id: &str,
        station_key_id: Option<&str>,
        range_start_ms: i64,
        range_end_ms: i64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<(), PersistenceError> {
        let overlapping = sqlx::query(
            r#"
            SELECT id, range_start_ms, range_end_ms
            FROM channel_monitor_rollup_dirty_ranges
            WHERE monitor_id = ?1
              AND (station_key_id IS ?2 OR station_key_id = ?2)
              AND reason = ?3
              AND range_start_ms <= ?4
              AND range_end_ms >= ?5
            ORDER BY created_at_ms ASC, id ASC
            "#,
        )
        .bind(monitor_id)
        .bind(station_key_id)
        .bind(reason)
        .bind(range_end_ms)
        .bind(range_start_ms)
        .fetch_all(&mut *connection)
        .await?;

        if overlapping.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO channel_monitor_rollup_dirty_ranges (
                    id, monitor_id, station_key_id, range_start_ms,
                    range_end_ms, reason, created_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(id)
            .bind(monitor_id)
            .bind(station_key_id)
            .bind(range_start_ms)
            .bind(range_end_ms)
            .bind(reason)
            .bind(created_at_ms)
            .execute(&mut *connection)
            .await?;
        } else {
            let primary_id: String = overlapping[0].get("id");
            let merged_start = overlapping.iter().fold(range_start_ms, |acc, row| {
                acc.min(row.get::<i64, _>("range_start_ms"))
            });
            let merged_end = overlapping.iter().fold(range_end_ms, |acc, row| {
                acc.max(row.get::<i64, _>("range_end_ms"))
            });
            for row in overlapping.iter().skip(1) {
                let redundant_id: String = row.get("id");
                sqlx::query("DELETE FROM channel_monitor_rollup_dirty_ranges WHERE id = ?1")
                    .bind(redundant_id)
                    .execute(&mut *connection)
                    .await?;
            }
            sqlx::query(
                r#"
                UPDATE channel_monitor_rollup_dirty_ranges
                SET range_start_ms = ?1, range_end_ms = ?2, created_at_ms = ?3
                WHERE id = ?4
                "#,
            )
            .bind(merged_start)
            .bind(merged_end)
            .bind(created_at_ms)
            .bind(primary_id)
            .execute(&mut *connection)
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn list_dirty_ranges(
        &self,
        connection: &mut SqliteConnection,
        limit: u32,
    ) -> Result<Vec<RollupDirtyRangeRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, monitor_id, station_key_id, range_start_ms, range_end_ms, reason
            FROM channel_monitor_rollup_dirty_ranges
            ORDER BY created_at_ms ASC, id ASC
            LIMIT ?1
            "#,
        )
        .bind(i64::from(limit.clamp(1, 1_000)))
        .fetch_all(connection)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| RollupDirtyRangeRow {
                id: row.get("id"),
                monitor_id: row.get("monitor_id"),
                station_key_id: row.get("station_key_id"),
                range_start_ms: row.get("range_start_ms"),
                range_end_ms: row.get("range_end_ms"),
                reason: row.get("reason"),
            })
            .collect())
    }

    pub(crate) async fn repair_dirty_ranges(
        &self,
        connection: &mut SqliteConnection,
        limit: u32,
        now_ms: i64,
    ) -> Result<RollupRepairOutcome, PersistenceError> {
        let ranges = self.list_dirty_ranges(connection, limit).await?;
        let mut repaired_ranges = 0_u32;
        let mut written_rollups = 0_u32;

        for range in ranges {
            written_rollups = written_rollups.saturating_add(
                self.rebuild_rollups_for_range(
                    connection,
                    &range.monitor_id,
                    range.station_key_id.as_deref(),
                    range.range_start_ms,
                    range.range_end_ms,
                    now_ms,
                )
                .await?,
            );
            sqlx::query("DELETE FROM channel_monitor_rollup_dirty_ranges WHERE id = ?1")
                .bind(&range.id)
                .execute(&mut *connection)
                .await?;
            repaired_ranges = repaired_ranges.saturating_add(1);
        }

        Ok(RollupRepairOutcome {
            repaired_ranges,
            written_rollups,
        })
    }

    pub(crate) async fn rebuild_rollups_for_range(
        &self,
        connection: &mut SqliteConnection,
        monitor_id: &str,
        station_key_id: Option<&str>,
        range_start_ms: i64,
        range_end_ms: i64,
        now_ms: i64,
    ) -> Result<u32, PersistenceError> {
        let query_start_ms = floor_ms(range_start_ms, 3_600_000);
        let query_end_ms = ceil_ms(range_end_ms, 86_400_000);
        let rows = sqlx::query(
            r#"
            SELECT station_key_id, finished_at_ms, terminal_outcome, terminal_failure_kind, latency_ms
            FROM channel_monitor_target_results
            WHERE monitor_id = ?1
              AND (station_key_id IS ?2 OR station_key_id = ?2)
              AND finished_at_ms IS NOT NULL
              AND finished_at_ms >= ?3
              AND finished_at_ms < ?4
            ORDER BY finished_at_ms ASC, id ASC
            "#,
        )
        .bind(monitor_id)
        .bind(station_key_id)
        .bind(query_start_ms)
        .bind(query_end_ms)
        .fetch_all(&mut *connection)
        .await?;

        let mut aggregates: BTreeMap<(Option<String>, &'static str, i64), RollupAggregate> =
            BTreeMap::new();
        for row in rows {
            let key_id = row.get::<Option<String>, _>("station_key_id");
            let finished_at_ms = row.get::<i64, _>("finished_at_ms");
            for (bucket_kind, bucket_start_ms, bucket_end_ms) in [
                ("hour", floor_ms(finished_at_ms, 3_600_000), 3_600_000),
                ("day", floor_ms(finished_at_ms, 86_400_000), 86_400_000),
            ] {
                let aggregate = aggregates
                    .entry((key_id.clone(), bucket_kind, bucket_start_ms))
                    .or_insert_with(|| RollupAggregate {
                        bucket_end_ms: bucket_start_ms + bucket_end_ms,
                        ..RollupAggregate::default()
                    });
                aggregate.push(
                    row.get::<String, _>("terminal_outcome").as_str(),
                    row.get::<Option<String>, _>("terminal_failure_kind")
                        .as_deref(),
                    row.get::<Option<i64>, _>("latency_ms"),
                );
            }
        }

        let mut written = 0_u32;
        for ((key_id, bucket_kind, bucket_start_ms), aggregate) in aggregates {
            sqlx::query(
                r#"
                INSERT INTO channel_monitor_bucket_rollups (
                    id, monitor_id, station_key_id, bucket_kind, bucket_start_ms,
                    bucket_end_ms, total_count, available_count, degraded_count,
                    unavailable_count, skipped_count, excluded_count, exclusion_counts_json,
                    failure_counts_json,
                    p50_latency_ms, p95_latency_ms, updated_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                ON CONFLICT(monitor_id, station_key_id, bucket_kind, bucket_start_ms)
                DO UPDATE SET
                    bucket_end_ms = excluded.bucket_end_ms,
                    total_count = excluded.total_count,
                    available_count = excluded.available_count,
                    degraded_count = excluded.degraded_count,
                    unavailable_count = excluded.unavailable_count,
                    skipped_count = excluded.skipped_count,
                    excluded_count = excluded.excluded_count,
                    exclusion_counts_json = excluded.exclusion_counts_json,
                    failure_counts_json = excluded.failure_counts_json,
                    p50_latency_ms = excluded.p50_latency_ms,
                    p95_latency_ms = excluded.p95_latency_ms,
                    updated_at_ms = excluded.updated_at_ms
                "#,
            )
            .bind(format!(
                "rollup:{monitor_id}:{}:{bucket_kind}:{bucket_start_ms}",
                key_id.as_deref().unwrap_or("station")
            ))
            .bind(monitor_id)
            .bind(&key_id)
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .bind(aggregate.bucket_end_ms)
            .bind(aggregate.total_count)
            .bind(aggregate.available_count)
            .bind(aggregate.degraded_count)
            .bind(aggregate.unavailable_count)
            .bind(aggregate.skipped_count)
            .bind(aggregate.excluded_count)
            .bind(aggregate.exclusion_counts_json())
            .bind(aggregate.failure_counts_json())
            .bind(aggregate.percentile_latency(50))
            .bind(aggregate.percentile_latency(95))
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
            written = written.saturating_add(1);
        }

        Ok(written)
    }

    pub(crate) async fn mark_corrupt_rollups_dirty(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
    ) -> Result<u32, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, monitor_id, station_key_id, bucket_start_ms, bucket_end_ms, failure_counts_json
            FROM channel_monitor_bucket_rollups
            "#,
        )
        .fetch_all(&mut *connection)
        .await?;

        let mut dirty = 0_u32;
        for row in rows {
            let failure_counts_json: String = row.get("failure_counts_json");
            let parsed = serde_json::from_str::<BTreeMap<String, u32>>(&failure_counts_json);
            if parsed.is_ok() {
                continue;
            }
            dirty = dirty.saturating_add(1);
            let station_key_id = row.get::<Option<String>, _>("station_key_id");
            self.mark_dirty_range(
                connection,
                &format!("dirty-corrupt-rollup-{}", row.get::<String, _>("id")),
                &row.get::<String, _>("monitor_id"),
                station_key_id.as_deref(),
                row.get("bucket_start_ms"),
                row.get("bucket_end_ms"),
                "corrupt_failure_counts",
                now_ms,
            )
            .await?;
        }
        Ok(dirty)
    }

    pub(crate) async fn delete_rolled_up_raw_executions(
        &self,
        connection: &mut SqliteConnection,
        cutoff_ms: i64,
        per_monitor_limit: u32,
        global_limit: u32,
    ) -> Result<RetentionDeleteOutcome, PersistenceError> {
        let candidates = sqlx::query(
            r#"
            SELECT e.id
            FROM channel_monitor_executions e
            WHERE e.status IN ('completed', 'partial', 'skipped', 'cancelled', 'interrupted')
              AND e.finished_at_ms IS NOT NULL
              AND e.finished_at_ms < ?1
              AND (
                SELECT COUNT(*)
                FROM channel_monitor_executions newer
                WHERE newer.monitor_id = e.monitor_id
                  AND newer.finished_at_ms IS NOT NULL
                  AND newer.finished_at_ms < ?1
                  AND (newer.finished_at_ms > e.finished_at_ms OR (newer.finished_at_ms = e.finished_at_ms AND newer.id > e.id))
              ) < ?2
              AND NOT EXISTS (
                SELECT 1
                FROM channel_monitor_target_results tr
                JOIN channel_monitor_rollup_dirty_ranges dr
                  ON dr.monitor_id = tr.monitor_id
                 AND (dr.station_key_id IS tr.station_key_id OR dr.station_key_id = tr.station_key_id)
                 AND tr.finished_at_ms >= dr.range_start_ms
                 AND tr.finished_at_ms < dr.range_end_ms
                WHERE tr.execution_id = e.id
              )
              AND NOT EXISTS (
                SELECT 1
                FROM channel_monitor_target_results tr
                WHERE tr.execution_id = e.id
                  AND tr.finished_at_ms IS NOT NULL
                  AND NOT EXISTS (
                    SELECT 1
                    FROM channel_monitor_bucket_rollups br
                    WHERE br.monitor_id = tr.monitor_id
                      AND (br.station_key_id IS tr.station_key_id OR br.station_key_id = tr.station_key_id)
                      AND br.bucket_kind = 'hour'
                      AND tr.finished_at_ms >= br.bucket_start_ms
                      AND tr.finished_at_ms < br.bucket_end_ms
                  )
              )
            ORDER BY e.finished_at_ms ASC, e.id ASC
            LIMIT ?3
            "#,
        )
        .bind(cutoff_ms)
        .bind(i64::from(per_monitor_limit.clamp(1, global_limit.max(1))))
        .bind(i64::from(global_limit.clamp(1, 50_000)))
        .fetch_all(&mut *connection)
        .await?;

        let mut deleted = 0_u32;
        for row in candidates {
            let execution_id: String = row.get("id");
            let affected = sqlx::query("DELETE FROM channel_monitor_executions WHERE id = ?1")
                .bind(execution_id)
                .execute(&mut *connection)
                .await?
                .rows_affected();
            if affected > 0 {
                deleted = deleted.saturating_add(1);
            }
        }

        Ok(RetentionDeleteOutcome {
            deleted_executions: deleted,
        })
    }
}

#[derive(Debug, Default)]
struct RollupAggregate {
    bucket_end_ms: i64,
    total_count: i64,
    available_count: i64,
    degraded_count: i64,
    unavailable_count: i64,
    skipped_count: i64,
    excluded_count: i64,
    exclusion_counts: BTreeMap<String, i64>,
    failure_counts: BTreeMap<String, i64>,
    latencies: Vec<i64>,
}

impl RollupAggregate {
    fn push(&mut self, outcome: &str, failure_kind: Option<&str>, latency_ms: Option<i64>) {
        if let Some(failure_kind) = failure_kind.filter(|kind| {
            matches!(
                *kind,
                "budget_exceeded"
                    | "balance_depleted"
                    | "quota_exhausted"
                    | "subscription_unavailable"
                    | "cancelled"
                    | "interrupted"
                    | "needs_configuration"
                    | "local_configuration"
                    | "local_budget"
                    | "local_internal_before_send"
            )
        }) {
            self.excluded_count += 1;
            *self
                .exclusion_counts
                .entry(failure_kind.to_string())
                .or_default() += 1;
            *self
                .failure_counts
                .entry(failure_kind.to_string())
                .or_default() += 1;
            return;
        }
        match outcome {
            "available" => {
                self.total_count += 1;
                self.available_count += 1;
            }
            "degraded" => {
                self.total_count += 1;
                self.degraded_count += 1;
            }
            "unavailable" => {
                self.total_count += 1;
                self.unavailable_count += 1;
            }
            "skipped" => self.skipped_count += 1,
            _ => {}
        }
        if matches!(outcome, "degraded" | "unavailable") {
            if let Some(failure_kind) = failure_kind.filter(|kind| !kind.trim().is_empty()) {
                *self
                    .failure_counts
                    .entry(failure_kind.to_string())
                    .or_insert(0) += 1;
            }
        }
        if let Some(latency_ms) = latency_ms.filter(|latency_ms| *latency_ms >= 0) {
            self.latencies.push(latency_ms);
        }
    }

    fn failure_counts_json(&self) -> String {
        json!(self.failure_counts).to_string()
    }

    fn exclusion_counts_json(&self) -> String {
        json!(self.exclusion_counts).to_string()
    }

    fn percentile_latency(&self, percentile: u32) -> Option<i64> {
        if self.latencies.is_empty() {
            return None;
        }
        let mut latencies = self.latencies.clone();
        latencies.sort_unstable();
        let rank = ((latencies.len() - 1) as u32).saturating_mul(percentile.min(100)) / 100;
        latencies.get(rank as usize).copied()
    }
}

fn floor_ms(value: i64, unit_ms: i64) -> i64 {
    value.div_euclid(unit_ms) * unit_ms
}

fn ceil_ms(value: i64, unit_ms: i64) -> i64 {
    floor_ms(value.saturating_sub(1), unit_ms).saturating_add(unit_ms)
}
