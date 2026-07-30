use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite};

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        monitoring::buckets::{
            hourly_bucket_windows, local_day_bucket_windows, recent_target_result_limit,
            BucketAvailabilityState, BucketCounts, BucketTimezoneSource, BucketWindow,
            BucketWindowKind,
        },
    },
    models::monitoring::{
        ChannelMonitorAttemptCursor, ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
        ChannelMonitorAttemptRecord, ChannelMonitorExecutionCursor, ChannelMonitorExecutionDetail,
        ChannelMonitorExecutionIdInput, ChannelMonitorExecutionListInput,
        ChannelMonitorExecutionPage, ChannelMonitorExecutionSummaryV2,
        ChannelMonitorTargetResultRecord, ChannelStatusAggregate, ChannelStatusBucket,
        ChannelStatusBucketBoundary, ChannelStatusBucketCounts, ChannelStatusBucketKind,
        ChannelStatusBucketLayout, ChannelStatusBucketState, ChannelStatusCursor,
        ChannelStatusFreshness, ChannelStatusLatestResult, ChannelStatusMonitor,
        ChannelStatusOutcome, ChannelStatusPage, ChannelStatusRecentPoint, ChannelStatusRow,
        ChannelStatusRunningExecution, ChannelStatusSortDirection, ChannelStatusSortField,
        ChannelStatusTarget, ChannelStatusTimezone, ChannelStatusTimezoneSource,
        ChannelStatusWindowSummaryV2, ChannelStatusWorkspaceInput, ChannelStatusWorkspaceV2,
        ChannelStatusWorkspaceWindow,
    },
    persistence::{error::PersistenceError, runtime::PersistenceHandle, ReadSession},
};

const WORKSPACE_SCHEMA_VERSION: u32 = 2;
const DEFAULT_WORKSPACE_LIMIT: u32 = 200;
const MAX_WORKSPACE_LIMIT: u32 = 500;
const DEFAULT_EXECUTION_LIMIT: u32 = 100;
const MAX_EXECUTION_LIMIT: u32 = 200;
const DEFAULT_ATTEMPT_LIMIT: u32 = 100;
const MAX_ATTEMPT_LIMIT: u32 = 200;
const MAX_BASE_SCAN_ROWS: i64 = 5_000;
const HOURLY_BUCKET_COUNT: u32 = 24;
const DAILY_BUCKET_COUNT: u32 = 30;
const DEGRADED_WEIGHT_BPS: u32 = 5_000;

type RowKey = (String, Option<String>);

#[derive(Clone)]
pub(crate) struct ChannelStatusReadModelQuery {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
}

impl ChannelStatusReadModelQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self { runtime, clock }
    }

    pub(crate) async fn load_workspace(
        &self,
        input: ChannelStatusWorkspaceInput,
    ) -> Result<ChannelStatusWorkspaceV2, ApplicationError> {
        let now_ms = self.clock.now_utc().timestamp_millis();
        let limit = input
            .limit
            .unwrap_or(DEFAULT_WORKSPACE_LIMIT)
            .clamp(1, MAX_WORKSPACE_LIMIT);
        let hourly_windows = hourly_bucket_windows(now_ms, HOURLY_BUCKET_COUNT);
        let daily_windows =
            local_day_bucket_windows(now_ms, DAILY_BUCKET_COUNT, input.timezone_id.as_deref());

        let mut read = self.runtime.begin_read().await?;
        let base_rows = load_base_rows(&mut read, &input).await?;
        let row_keys = base_rows
            .iter()
            .map(|row| (row.monitor_id.clone(), row.station_key_id.clone()))
            .collect::<Vec<_>>();
        let recent = load_recent_results(&mut read, &row_keys).await?;
        let running = load_running_executions(&mut read, &row_keys).await?;
        let hourly_rollups =
            load_rollups(&mut read, &row_keys, "hour", &hourly_windows.windows).await?;
        let daily_rollups =
            load_rollups(&mut read, &row_keys, "day", &daily_windows.windows).await?;
        let dirty_ranges = load_dirty_ranges(
            &mut read,
            &row_keys,
            min_window_start(&hourly_windows.windows, &daily_windows.windows),
            max_window_end(&hourly_windows.windows, &daily_windows.windows),
        )
        .await?;

        let mut rows = base_rows
            .into_iter()
            .map(|base| {
                let key = (base.monitor_id.clone(), base.station_key_id.clone());
                let recent_points = recent.get(&key).cloned().unwrap_or_default();
                let latest = recent_points.first().map(latest_from_recent);
                let hourly_buckets = build_buckets(
                    &hourly_windows.windows,
                    ChannelStatusBucketKind::Hour,
                    hourly_rollups.get(&key),
                    dirty_ranges.get(&key),
                );
                let daily_buckets = build_buckets(
                    &daily_windows.windows,
                    ChannelStatusBucketKind::Day,
                    daily_rollups.get(&key),
                    dirty_ranges.get(&key),
                );
                let running = running.get(&key).cloned();
                let selected_window = summarize_selected_window(
                    input.window,
                    &recent_points,
                    &hourly_buckets,
                    &daily_buckets,
                    latest.as_ref(),
                );
                ChannelStatusRow {
                    row_key: row_key(&key.0, key.1.as_deref()),
                    monitor: ChannelStatusMonitor {
                        id: base.monitor_id,
                        name: base.monitor_name,
                        target_type: base.target_type,
                        enabled: base.enabled,
                        protocol_kind: base.protocol_kind,
                        client_profile_id: base.client_profile_id,
                        client_profile_version: base.client_profile_version,
                        primary_model: base.primary_model,
                        fallback_models: base.fallback_models,
                        interval_seconds: base.interval_seconds,
                        jitter_seconds: base.jitter_seconds,
                        next_due_at_ms: base.next_due_at_ms,
                    },
                    target: ChannelStatusTarget {
                        station_id: base.station_id,
                        station_name: base.station_name,
                        station_key_id: base.station_key_id,
                        station_key_name: base.station_key_name,
                    },
                    latest,
                    running,
                    recent: recent_points,
                    hourly_buckets,
                    daily_buckets,
                    selected_window,
                }
            })
            .filter(|row| {
                input
                    .filter
                    .outcome
                    .is_none_or(|outcome| row.selected_window.latest_outcome == outcome)
            })
            .collect::<Vec<_>>();

        sort_rows(&mut rows, input.sort.field, input.sort.direction);
        let total_rows = rows.len() as u32;
        let start_index = input
            .cursor
            .as_ref()
            .and_then(|cursor| {
                rows.iter()
                    .position(|row| row.row_key == cursor.row_key)
                    .map(|index| index.saturating_add(1))
            })
            .unwrap_or(0);
        let mut page_rows = rows
            .into_iter()
            .skip(start_index)
            .take(limit.saturating_add(1) as usize)
            .collect::<Vec<_>>();
        let next_cursor = if page_rows.len() > limit as usize {
            page_rows.pop().map(|row| ChannelStatusCursor {
                row_key: row.row_key,
            })
        } else {
            None
        };
        let aggregate = aggregate_rows(total_rows, &page_rows);
        let freshness = freshness(&page_rows);

        Ok(ChannelStatusWorkspaceV2 {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            generated_at_ms: now_ms,
            window: input.window,
            timezone: timezone_from_bucket_set(&daily_windows),
            bucket_layout: ChannelStatusBucketLayout {
                recent_limit: recent_target_result_limit(),
                hourly: boundaries(&hourly_windows.windows, ChannelStatusBucketKind::Hour),
                daily: boundaries(&daily_windows.windows, ChannelStatusBucketKind::Day),
            },
            aggregate,
            freshness,
            page: ChannelStatusPage {
                limit,
                returned: page_rows.len() as u32,
                next_cursor,
            },
            rows: page_rows,
        })
    }

    pub(crate) async fn list_executions(
        &self,
        input: ChannelMonitorExecutionListInput,
    ) -> Result<ChannelMonitorExecutionPage, ApplicationError> {
        let limit = input
            .limit
            .unwrap_or(DEFAULT_EXECUTION_LIMIT)
            .clamp(1, MAX_EXECUTION_LIMIT);
        let mut read = self.runtime.begin_read().await?;
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT DISTINCT
                e.id,
                e.monitor_id,
                e.status,
                e.trigger_kind,
                e.trigger_request_id,
                e.planned_at_ms,
                e.started_at_ms,
                e.finished_at_ms,
                e.target_count,
                e.available_count,
                e.degraded_count,
                e.unavailable_count,
                e.skipped_count,
                e.summary_outcome,
                e.summary_failure_kind,
                e.created_at_ms
            FROM channel_monitor_executions e
            "#,
        );
        if input.station_key_id.is_some() {
            query.push(" JOIN channel_monitor_target_results tr ON tr.execution_id = e.id ");
        }
        query.push(" WHERE 1 = 1 ");
        if let Some(monitor_id) = normalized_filter(input.monitor_id.as_deref()) {
            query.push(" AND e.monitor_id = ");
            query.push_bind(monitor_id);
        }
        if let Some(station_key_id) = normalized_filter(input.station_key_id.as_deref()) {
            query.push(" AND tr.station_key_id = ");
            query.push_bind(station_key_id);
        }
        if let Some(status) = normalized_filter(input.status.as_deref()) {
            query.push(" AND e.status = ");
            query.push_bind(status);
        }
        if let Some(cursor) = &input.cursor {
            query.push(" AND (COALESCE(e.started_at_ms, e.planned_at_ms) < ");
            query.push_bind(cursor.started_at_ms);
            query.push(" OR (COALESCE(e.started_at_ms, e.planned_at_ms) = ");
            query.push_bind(cursor.started_at_ms);
            query.push(" AND e.id < ");
            query.push_bind(&cursor.execution_id);
            query.push("))");
        }
        query.push(" ORDER BY COALESCE(e.started_at_ms, e.planned_at_ms) DESC, e.id DESC LIMIT ");
        query.push_bind(i64::from(limit.saturating_add(1)));

        let rows = query
            .build()
            .fetch_all(read.connection())
            .await
            .map_err(PersistenceError::from)?;
        let mut items = rows
            .into_iter()
            .map(execution_summary_from_row)
            .collect::<Vec<_>>();
        let next_cursor = if items.len() > limit as usize {
            items.pop().map(|item| ChannelMonitorExecutionCursor {
                started_at_ms: item.started_at_ms.unwrap_or(item.planned_at_ms),
                execution_id: item.execution_id,
            })
        } else {
            None
        };
        Ok(ChannelMonitorExecutionPage { items, next_cursor })
    }

    pub(crate) async fn get_execution(
        &self,
        input: ChannelMonitorExecutionIdInput,
    ) -> Result<ChannelMonitorExecutionDetail, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let execution_row = sqlx::query(
            r#"
            SELECT id, monitor_id, status, trigger_kind, trigger_request_id,
                   planned_at_ms, started_at_ms, finished_at_ms, target_count,
                   available_count, degraded_count, unavailable_count, skipped_count,
                   summary_outcome, summary_failure_kind, created_at_ms
            FROM channel_monitor_executions
            WHERE id = ?1
            "#,
        )
        .bind(&input.execution_id)
        .fetch_one(read.connection())
        .await
        .map_err(PersistenceError::from)?;
        let execution = execution_summary_from_row(execution_row);
        let target_rows = sqlx::query(
            r#"
            SELECT id, execution_id, monitor_id, station_id, station_key_id,
                   terminal_outcome, terminal_failure_kind, terminal_reason,
                   requested_model, effective_model, used_fallback, attempt_count,
                   decisive_attempt_id, protocol_kind, resolved_adapter_kind,
                   resolved_dialect, client_profile_id, client_profile_version,
                   request_profile_hash, traffic_equivalence, health_writeback_mode,
                   health_writeback_decision, health_writeback_reason, latency_ms,
                   semantic_confidence, started_at_ms, finished_at_ms
            FROM channel_monitor_target_results
            WHERE execution_id = ?1
            ORDER BY station_key_id ASC, id ASC
            "#,
        )
        .bind(&input.execution_id)
        .fetch_all(read.connection())
        .await
        .map_err(PersistenceError::from)?;
        Ok(ChannelMonitorExecutionDetail {
            execution,
            targets: target_rows
                .into_iter()
                .map(target_result_from_row)
                .collect(),
        })
    }

    pub(crate) async fn list_attempt_history(
        &self,
        input: ChannelMonitorAttemptHistoryInput,
    ) -> Result<ChannelMonitorAttemptPage, ApplicationError> {
        let limit = input
            .limit
            .unwrap_or(DEFAULT_ATTEMPT_LIMIT)
            .clamp(1, MAX_ATTEMPT_LIMIT);
        let mut read = self.runtime.begin_read().await?;
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, execution_id, monitor_id, station_id, station_key_id,
                   model, model_role, model_index, attempt_number, protocol_kind,
                   client_profile_id, client_profile_version, request_profile_hash,
                   transport_mode, started_at_ms, finished_at_ms, latency_ms,
                   http_status, outcome, failure_kind, retryable, response_model,
                   content_extracted, validation_passed, output_bytes, error_summary
            FROM channel_monitor_attempts
            WHERE execution_id =
            "#,
        );
        query.push_bind(&input.execution_id);
        if let Some(station_key_id) = normalized_filter(input.station_key_id.as_deref()) {
            query.push(" AND station_key_id = ");
            query.push_bind(station_key_id);
        }
        if let Some(cursor) = &input.cursor {
            query.push(" AND (started_at_ms > ");
            query.push_bind(cursor.started_at_ms);
            query.push(" OR (started_at_ms = ");
            query.push_bind(cursor.started_at_ms);
            query.push(" AND id > ");
            query.push_bind(&cursor.attempt_id);
            query.push("))");
        }
        query.push(" ORDER BY started_at_ms ASC, id ASC LIMIT ");
        query.push_bind(i64::from(limit.saturating_add(1)));

        let rows = query
            .build()
            .fetch_all(read.connection())
            .await
            .map_err(PersistenceError::from)?;
        let mut items = rows.into_iter().map(attempt_from_row).collect::<Vec<_>>();
        let next_cursor = if items.len() > limit as usize {
            items.pop().map(|item| ChannelMonitorAttemptCursor {
                started_at_ms: item.started_at_ms,
                attempt_id: item.attempt_id,
            })
        } else {
            None
        };
        Ok(ChannelMonitorAttemptPage { items, next_cursor })
    }
}

#[derive(Debug, Clone)]
struct BaseStatusRow {
    monitor_id: String,
    monitor_name: String,
    target_type: String,
    station_id: String,
    station_name: Option<String>,
    station_key_id: Option<String>,
    station_key_name: Option<String>,
    enabled: bool,
    protocol_kind: String,
    client_profile_id: String,
    client_profile_version: i64,
    primary_model: String,
    fallback_models: Vec<String>,
    interval_seconds: i64,
    jitter_seconds: i64,
    next_due_at_ms: Option<i64>,
}

#[derive(Debug, Clone)]
struct RollupCell {
    counts: ChannelStatusBucketCounts,
    failure_counts: BTreeMap<String, u32>,
    p50_latency_ms: Option<i64>,
    p95_latency_ms: Option<i64>,
    dirty: bool,
    corrupt: bool,
}

#[derive(Debug, Clone)]
struct DirtyRange {
    start_ms: i64,
    end_ms: i64,
}

fn execution_summary_from_row(row: SqliteRow) -> ChannelMonitorExecutionSummaryV2 {
    ChannelMonitorExecutionSummaryV2 {
        execution_id: row.get("id"),
        monitor_id: row.get("monitor_id"),
        status: row.get("status"),
        trigger_kind: row.get("trigger_kind"),
        trigger_request_id: row.get("trigger_request_id"),
        planned_at_ms: row.get("planned_at_ms"),
        started_at_ms: row.get("started_at_ms"),
        finished_at_ms: row.get("finished_at_ms"),
        target_count: row.get("target_count"),
        available_count: row.get("available_count"),
        degraded_count: row.get("degraded_count"),
        unavailable_count: row.get("unavailable_count"),
        skipped_count: row.get("skipped_count"),
        summary_outcome: row.get("summary_outcome"),
        summary_failure_kind: row.get("summary_failure_kind"),
        created_at_ms: row.get("created_at_ms"),
    }
}

fn target_result_from_row(row: SqliteRow) -> ChannelMonitorTargetResultRecord {
    ChannelMonitorTargetResultRecord {
        target_result_id: row.get("id"),
        execution_id: row.get("execution_id"),
        monitor_id: row.get("monitor_id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        terminal_outcome: row.get("terminal_outcome"),
        terminal_failure_kind: row.get("terminal_failure_kind"),
        terminal_reason: row.get("terminal_reason"),
        requested_model: row.get("requested_model"),
        effective_model: row.get("effective_model"),
        used_fallback: row.get::<i64, _>("used_fallback") != 0,
        attempt_count: row.get("attempt_count"),
        decisive_attempt_id: row.get("decisive_attempt_id"),
        protocol_kind: row.get("protocol_kind"),
        resolved_adapter_kind: row.get("resolved_adapter_kind"),
        resolved_dialect: row.get("resolved_dialect"),
        client_profile_id: row.get("client_profile_id"),
        client_profile_version: row.get("client_profile_version"),
        request_profile_hash: row.get("request_profile_hash"),
        traffic_equivalence: row.get("traffic_equivalence"),
        health_writeback_mode: row.get("health_writeback_mode"),
        health_writeback_decision: row.get("health_writeback_decision"),
        health_writeback_reason: row.get("health_writeback_reason"),
        latency_ms: row.get("latency_ms"),
        semantic_confidence: row.get("semantic_confidence"),
        started_at_ms: row.get("started_at_ms"),
        finished_at_ms: row.get("finished_at_ms"),
    }
}

fn attempt_from_row(row: SqliteRow) -> ChannelMonitorAttemptRecord {
    ChannelMonitorAttemptRecord {
        attempt_id: row.get("id"),
        execution_id: row.get("execution_id"),
        monitor_id: row.get("monitor_id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        model: row.get("model"),
        model_role: row.get("model_role"),
        model_index: row.get("model_index"),
        attempt_number: row.get("attempt_number"),
        protocol_kind: row.get("protocol_kind"),
        client_profile_id: row.get("client_profile_id"),
        client_profile_version: row.get("client_profile_version"),
        request_profile_hash: row.get("request_profile_hash"),
        transport_mode: row.get("transport_mode"),
        started_at_ms: row.get("started_at_ms"),
        finished_at_ms: row.get("finished_at_ms"),
        latency_ms: row.get("latency_ms"),
        http_status: row.get("http_status"),
        outcome: row.get("outcome"),
        failure_kind: row.get("failure_kind"),
        retryable: row.get::<i64, _>("retryable") != 0,
        response_model: row.get("response_model"),
        content_extracted: row.get::<i64, _>("content_extracted") != 0,
        validation_passed: row.get::<i64, _>("validation_passed") != 0,
        output_bytes: row.get("output_bytes"),
        error_summary: row.get("error_summary"),
    }
}

async fn load_base_rows(
    read: &mut ReadSession,
    input: &ChannelStatusWorkspaceInput,
) -> Result<Vec<BaseStatusRow>, ApplicationError> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            m.id AS monitor_id,
            m.name AS monitor_name,
            m.target_type,
            m.station_id,
            s.name AS station_name,
            sk.id AS station_key_id,
            sk.name AS station_key_name,
            m.enabled,
            m.protocol_kind,
            m.client_profile_id,
            m.client_profile_version,
            m.primary_model,
            m.fallback_models_v2_json,
            m.interval_seconds,
            m.jitter_seconds,
            m.next_due_at_ms
        FROM channel_monitors m
        LEFT JOIN stations s ON s.id = m.station_id
        LEFT JOIN station_keys sk
          ON (
            (m.target_type = 'station_key' AND sk.id = m.station_key_id)
            OR (m.target_type = 'station' AND sk.station_id = m.station_id)
          )
        WHERE 1 = 1
        "#,
    );
    if let Some(enabled) = input.filter.enabled {
        query.push(" AND m.enabled = ");
        query.push_bind(if enabled { 1_i64 } else { 0_i64 });
    }
    if let Some(station_id) = normalized_filter(input.filter.station_id.as_deref()) {
        query.push(" AND m.station_id = ");
        query.push_bind(station_id);
    }
    if let Some(protocol_kind) = normalized_filter(input.filter.protocol_kind.as_deref()) {
        query.push(" AND m.protocol_kind = ");
        query.push_bind(protocol_kind);
    }
    if let Some(client_profile_id) = normalized_filter(input.filter.client_profile_id.as_deref()) {
        query.push(" AND m.client_profile_id = ");
        query.push_bind(client_profile_id);
    }
    if let Some(search) = normalized_search(input.filter.search.as_deref()) {
        query.push(" AND (lower(m.name) LIKE lower(");
        query.push_bind(format!("%{search}%"));
        query.push(") OR lower(COALESCE(s.name, '')) LIKE lower(");
        query.push_bind(format!("%{search}%"));
        query.push(") OR lower(COALESCE(sk.name, '')) LIKE lower(");
        query.push_bind(format!("%{search}%"));
        query.push("))");
    }
    query.push(
        r#"
        ORDER BY lower(m.name) ASC, m.id ASC, lower(COALESCE(sk.name, '')) ASC, COALESCE(sk.id, '') ASC
        LIMIT
        "#,
    );
    query.push_bind(MAX_BASE_SCAN_ROWS);

    let rows = query
        .build()
        .fetch_all(read.connection())
        .await
        .map_err(PersistenceError::from)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let fallback_json: String = row.get("fallback_models_v2_json");
            BaseStatusRow {
                monitor_id: row.get("monitor_id"),
                monitor_name: row.get("monitor_name"),
                target_type: row.get("target_type"),
                station_id: row.get("station_id"),
                station_name: row.get("station_name"),
                station_key_id: row.get("station_key_id"),
                station_key_name: row.get("station_key_name"),
                enabled: row.get::<i64, _>("enabled") != 0,
                protocol_kind: row.get("protocol_kind"),
                client_profile_id: row.get("client_profile_id"),
                client_profile_version: row.get("client_profile_version"),
                primary_model: row.get("primary_model"),
                fallback_models: serde_json::from_str(&fallback_json).unwrap_or_default(),
                interval_seconds: row.get("interval_seconds"),
                jitter_seconds: row.get("jitter_seconds"),
                next_due_at_ms: row.get("next_due_at_ms"),
            }
        })
        .collect())
}

async fn load_recent_results(
    read: &mut ReadSession,
    row_keys: &[RowKey],
) -> Result<BTreeMap<RowKey, Vec<ChannelStatusRecentPoint>>, ApplicationError> {
    if row_keys.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut grouped = BTreeMap::<RowKey, Vec<ChannelStatusRecentPoint>>::new();
    let limit = i64::from(recent_target_result_limit());
    for (monitor_id, station_key_id) in row_keys {
        let rows = if let Some(station_key_id) = station_key_id {
            sqlx::query(
                r#"
                SELECT
                    id,
                    execution_id,
                    monitor_id,
                    station_key_id,
                    terminal_outcome,
                    terminal_failure_kind,
                    terminal_reason,
                    latency_ms,
                    finished_at_ms,
                    used_fallback,
                    semantic_confidence,
                    attempt_count,
                    effective_model
                FROM channel_monitor_target_results
                WHERE monitor_id = ?1
                  AND station_key_id = ?2
                  AND finished_at_ms IS NOT NULL
                ORDER BY finished_at_ms DESC, id DESC
                LIMIT ?3
                "#,
            )
            .bind(monitor_id)
            .bind(station_key_id)
            .bind(limit)
            .fetch_all(read.connection())
            .await
            .map_err(PersistenceError::from)?
        } else {
            sqlx::query(
                r#"
                SELECT
                    id,
                    execution_id,
                    monitor_id,
                    station_key_id,
                    terminal_outcome,
                    terminal_failure_kind,
                    terminal_reason,
                    latency_ms,
                    finished_at_ms,
                    used_fallback,
                    semantic_confidence,
                    attempt_count,
                    effective_model
                FROM channel_monitor_target_results
                WHERE monitor_id = ?1
                  AND station_key_id IS NULL
                  AND finished_at_ms IS NOT NULL
                ORDER BY finished_at_ms DESC, id DESC
                LIMIT ?2
                "#,
            )
            .bind(monitor_id)
            .bind(limit)
            .fetch_all(read.connection())
            .await
            .map_err(PersistenceError::from)?
        };

        let key = (monitor_id.clone(), station_key_id.clone());
        let points = grouped.entry(key).or_default();
        points.reserve(rows.len());
        for row in rows {
            points.push(ChannelStatusRecentPoint {
                target_result_id: row.get("id"),
                execution_id: row.get("execution_id"),
                outcome: ChannelStatusOutcome::from_probe_outcome(
                    row.get::<String, _>("terminal_outcome").as_str(),
                ),
                failure_kind: row.get("terminal_failure_kind"),
                terminal_reason: row.get("terminal_reason"),
                latency_ms: row.get("latency_ms"),
                checked_at_ms: row.get("finished_at_ms"),
                used_fallback: row.get::<i64, _>("used_fallback") != 0,
                semantic_confidence: row.get("semantic_confidence"),
                attempt_count: row.get("attempt_count"),
                effective_model: row.get("effective_model"),
            });
        }
    }
    Ok(grouped)
}

async fn load_running_executions(
    read: &mut ReadSession,
    row_keys: &[RowKey],
) -> Result<BTreeMap<RowKey, ChannelStatusRunningExecution>, ApplicationError> {
    if row_keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        WITH scoped(monitor_id, station_key_id) AS (
        "#,
    );
    push_scoped_values(&mut query, row_keys);
    query.push(
        r#"
        ),
        ranked AS (
            SELECT
                e.id AS execution_id,
                e.monitor_id,
                s.station_key_id,
                e.status,
                e.trigger_kind,
                e.trigger_request_id,
                e.planned_at_ms,
                e.started_at_ms,
                ROW_NUMBER() OVER (
                    PARTITION BY e.monitor_id, s.station_key_id
                    ORDER BY COALESCE(e.started_at_ms, e.planned_at_ms) DESC, e.id DESC
                ) AS rn
            FROM channel_monitor_executions e
            JOIN scoped s ON s.monitor_id = e.monitor_id
            WHERE e.status IN ('queued', 'running')
        )
        SELECT *
        FROM ranked
        WHERE rn = 1
        "#,
    );
    let rows = query
        .build()
        .fetch_all(read.connection())
        .await
        .map_err(PersistenceError::from)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                (
                    row.get::<String, _>("monitor_id"),
                    row.get::<Option<String>, _>("station_key_id"),
                ),
                ChannelStatusRunningExecution {
                    execution_id: row.get("execution_id"),
                    status: row.get("status"),
                    trigger_kind: row.get("trigger_kind"),
                    trigger_request_id: row.get("trigger_request_id"),
                    planned_at_ms: row.get("planned_at_ms"),
                    started_at_ms: row.get("started_at_ms"),
                },
            )
        })
        .collect())
}

async fn load_rollups(
    read: &mut ReadSession,
    row_keys: &[RowKey],
    bucket_kind: &str,
    windows: &[BucketWindow],
) -> Result<BTreeMap<RowKey, BTreeMap<i64, RollupCell>>, ApplicationError> {
    if row_keys.is_empty() || windows.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        WITH scoped(monitor_id, station_key_id) AS (
        "#,
    );
    push_scoped_values(&mut query, row_keys);
    query.push(
        r#"
        )
        SELECT
            br.monitor_id,
            br.station_key_id,
            br.bucket_start_ms,
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
        JOIN scoped s
          ON br.monitor_id = s.monitor_id
         AND (br.station_key_id IS s.station_key_id OR br.station_key_id = s.station_key_id)
        WHERE br.bucket_kind =
        "#,
    );
    query.push_bind(bucket_kind);
    query.push(" AND br.bucket_start_ms >= ");
    query.push_bind(
        windows
            .first()
            .map(|window| window.start_ms)
            .unwrap_or_default(),
    );
    query.push(" AND br.bucket_start_ms < ");
    query.push_bind(
        windows
            .last()
            .map(|window| window.end_ms)
            .unwrap_or_default(),
    );
    query.push(" ORDER BY br.monitor_id ASC, br.station_key_id ASC, br.bucket_start_ms ASC");

    let rows = query
        .build()
        .fetch_all(read.connection())
        .await
        .map_err(PersistenceError::from)?;
    let mut grouped = BTreeMap::<RowKey, BTreeMap<i64, RollupCell>>::new();
    for row in rows {
        let failure_counts_json: String = row.get("failure_counts_json");
        let parsed_failure_counts =
            serde_json::from_str::<BTreeMap<String, u32>>(&failure_counts_json);
        let key = (
            row.get::<String, _>("monitor_id"),
            row.get::<Option<String>, _>("station_key_id"),
        );
        grouped.entry(key).or_default().insert(
            row.get("bucket_start_ms"),
            RollupCell {
                counts: ChannelStatusBucketCounts {
                    total: clamp_count(row.get("total_count")),
                    available: clamp_count(row.get("available_count")),
                    degraded: clamp_count(row.get("degraded_count")),
                    unavailable: clamp_count(row.get("unavailable_count")),
                    skipped: clamp_count(row.get("skipped_count")),
                },
                failure_counts: parsed_failure_counts.unwrap_or_default(),
                p50_latency_ms: row.get("p50_latency_ms"),
                p95_latency_ms: row.get("p95_latency_ms"),
                dirty: row.get::<i64, _>("dirty") != 0,
                corrupt: serde_json::from_str::<BTreeMap<String, u32>>(&failure_counts_json)
                    .is_err(),
            },
        );
    }
    Ok(grouped)
}

async fn load_dirty_ranges(
    read: &mut ReadSession,
    row_keys: &[RowKey],
    range_start_ms: i64,
    range_end_ms: i64,
) -> Result<BTreeMap<RowKey, Vec<DirtyRange>>, ApplicationError> {
    if row_keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        WITH scoped(monitor_id, station_key_id) AS (
        "#,
    );
    push_scoped_values(&mut query, row_keys);
    query.push(
        r#"
        )
        SELECT dr.monitor_id, dr.station_key_id, dr.range_start_ms, dr.range_end_ms
        FROM channel_monitor_rollup_dirty_ranges dr
        JOIN scoped s
          ON dr.monitor_id = s.monitor_id
         AND (dr.station_key_id IS s.station_key_id OR dr.station_key_id = s.station_key_id)
        WHERE dr.range_start_ms <
        "#,
    );
    query.push_bind(range_end_ms);
    query.push(" AND dr.range_end_ms > ");
    query.push_bind(range_start_ms);

    let rows = query
        .build()
        .fetch_all(read.connection())
        .await
        .map_err(PersistenceError::from)?;
    let mut grouped = BTreeMap::<RowKey, Vec<DirtyRange>>::new();
    for row in rows {
        grouped
            .entry((
                row.get::<String, _>("monitor_id"),
                row.get::<Option<String>, _>("station_key_id"),
            ))
            .or_default()
            .push(DirtyRange {
                start_ms: row.get("range_start_ms"),
                end_ms: row.get("range_end_ms"),
            });
    }
    Ok(grouped)
}

fn push_scoped_values<'a>(query: &mut QueryBuilder<'a, Sqlite>, row_keys: &'a [RowKey]) {
    let mut separated = query.separated(", ");
    for (monitor_id, station_key_id) in row_keys {
        separated.push("(");
        separated.push_bind(monitor_id);
        separated.push(", ");
        separated.push_bind(station_key_id);
        separated.push(")");
    }
}

fn build_buckets(
    windows: &[BucketWindow],
    kind: ChannelStatusBucketKind,
    rollups: Option<&BTreeMap<i64, RollupCell>>,
    dirty_ranges: Option<&Vec<DirtyRange>>,
) -> Vec<ChannelStatusBucket> {
    windows
        .iter()
        .map(|window| {
            let dirty_without_rollup = dirty_ranges
                .is_some_and(|ranges| ranges.iter().any(|range| overlaps(window, range)));
            match rollups.and_then(|rollups| rollups.get(&window.start_ms)) {
                Some(cell) => bucket_from_rollup(window, kind, cell),
                None => ChannelStatusBucket {
                    kind,
                    start_ms: window.start_ms,
                    end_ms: window.end_ms,
                    state: if dirty_without_rollup {
                        ChannelStatusBucketState::Dirty
                    } else {
                        ChannelStatusBucketState::Missing
                    },
                    counts: ChannelStatusBucketCounts::default(),
                    strict_availability_bps: None,
                    effective_availability_bps: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                    failure_counts: BTreeMap::new(),
                    dirty: dirty_without_rollup,
                    corrupt: false,
                },
            }
        })
        .collect()
}

fn bucket_from_rollup(
    window: &BucketWindow,
    kind: ChannelStatusBucketKind,
    cell: &RollupCell,
) -> ChannelStatusBucket {
    let counts = bucket_counts(cell.counts);
    let state = if cell.dirty || cell.corrupt {
        ChannelStatusBucketState::Dirty
    } else {
        match counts.state() {
            BucketAvailabilityState::Missing => ChannelStatusBucketState::Missing,
            BucketAvailabilityState::SkippedOnly => ChannelStatusBucketState::SkippedOnly,
            BucketAvailabilityState::Available => ChannelStatusBucketState::Available,
            BucketAvailabilityState::Degraded => ChannelStatusBucketState::Degraded,
            BucketAvailabilityState::Unavailable => ChannelStatusBucketState::Unavailable,
        }
    };
    ChannelStatusBucket {
        kind,
        start_ms: window.start_ms,
        end_ms: window.end_ms,
        state,
        counts: cell.counts,
        strict_availability_bps: counts.strict_availability_bps(),
        effective_availability_bps: counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        p50_latency_ms: cell.p50_latency_ms,
        p95_latency_ms: cell.p95_latency_ms,
        failure_counts: cell.failure_counts.clone(),
        dirty: cell.dirty,
        corrupt: cell.corrupt,
    }
}

fn summarize_selected_window(
    window: ChannelStatusWorkspaceWindow,
    recent: &[ChannelStatusRecentPoint],
    hourly: &[ChannelStatusBucket],
    daily: &[ChannelStatusBucket],
    latest: Option<&ChannelStatusLatestResult>,
) -> ChannelStatusWindowSummaryV2 {
    match window {
        ChannelStatusWorkspaceWindow::Recent => summarize_recent_window(window, recent, latest),
        ChannelStatusWorkspaceWindow::Last24h => summarize_bucket_window(window, hourly),
        ChannelStatusWorkspaceWindow::Last7d => {
            summarize_bucket_window(window, select_tail(daily, 7))
        }
        ChannelStatusWorkspaceWindow::Last30d => summarize_bucket_window(window, daily),
    }
}

fn summarize_recent_window(
    window: ChannelStatusWorkspaceWindow,
    recent: &[ChannelStatusRecentPoint],
    latest: Option<&ChannelStatusLatestResult>,
) -> ChannelStatusWindowSummaryV2 {
    let counts = recent
        .iter()
        .fold(ChannelStatusBucketCounts::default(), |mut counts, point| {
            counts.total = counts.total.saturating_add(1);
            match point.outcome {
                ChannelStatusOutcome::Available => counts.available += 1,
                ChannelStatusOutcome::Degraded => counts.degraded += 1,
                ChannelStatusOutcome::Unavailable => counts.unavailable += 1,
                ChannelStatusOutcome::Skipped => counts.skipped += 1,
                ChannelStatusOutcome::Missing => {}
            }
            counts
        });
    let bucket_counts = bucket_counts(counts);
    ChannelStatusWindowSummaryV2 {
        window,
        bucket_kind: ChannelStatusBucketKind::Hour,
        start_ms: recent
            .last()
            .and_then(|point| point.checked_at_ms)
            .unwrap_or(0),
        end_ms: recent
            .first()
            .and_then(|point| point.checked_at_ms)
            .unwrap_or(0),
        counts,
        strict_availability_bps: bucket_counts.strict_availability_bps(),
        effective_availability_bps: bucket_counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        latest_outcome: latest
            .map(|latest| latest.outcome)
            .unwrap_or(ChannelStatusOutcome::Missing),
        latest_checked_at_ms: latest.and_then(|latest| latest.finished_at_ms),
        dirty: false,
        corrupt: false,
    }
}

fn summarize_bucket_window(
    window: ChannelStatusWorkspaceWindow,
    buckets: &[ChannelStatusBucket],
) -> ChannelStatusWindowSummaryV2 {
    let counts = buckets.iter().fold(
        ChannelStatusBucketCounts::default(),
        |mut counts, bucket| {
            counts.total = counts.total.saturating_add(bucket.counts.total);
            counts.available = counts.available.saturating_add(bucket.counts.available);
            counts.degraded = counts.degraded.saturating_add(bucket.counts.degraded);
            counts.unavailable = counts.unavailable.saturating_add(bucket.counts.unavailable);
            counts.skipped = counts.skipped.saturating_add(bucket.counts.skipped);
            counts
        },
    );
    let bucket_counts = bucket_counts(counts);
    let latest_bucket = buckets.iter().rev().find(|bucket| {
        !matches!(
            bucket.state,
            ChannelStatusBucketState::Missing | ChannelStatusBucketState::Dirty
        )
    });
    ChannelStatusWindowSummaryV2 {
        window,
        bucket_kind: buckets
            .first()
            .map(|bucket| bucket.kind)
            .unwrap_or(ChannelStatusBucketKind::Day),
        start_ms: buckets.first().map(|bucket| bucket.start_ms).unwrap_or(0),
        end_ms: buckets.last().map(|bucket| bucket.end_ms).unwrap_or(0),
        counts,
        strict_availability_bps: bucket_counts.strict_availability_bps(),
        effective_availability_bps: bucket_counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        latest_outcome: latest_bucket
            .map(bucket_outcome)
            .unwrap_or(ChannelStatusOutcome::Missing),
        latest_checked_at_ms: latest_bucket.map(|bucket| bucket.end_ms),
        dirty: buckets.iter().any(|bucket| bucket.dirty),
        corrupt: buckets.iter().any(|bucket| bucket.corrupt),
    }
}

fn latest_from_recent(point: &ChannelStatusRecentPoint) -> ChannelStatusLatestResult {
    ChannelStatusLatestResult {
        target_result_id: point.target_result_id.clone(),
        execution_id: point.execution_id.clone(),
        outcome: point.outcome,
        failure_kind: point.failure_kind.clone(),
        terminal_reason: point.terminal_reason.clone(),
        latency_ms: point.latency_ms,
        finished_at_ms: point.checked_at_ms,
        semantic_confidence: point.semantic_confidence.clone(),
        used_fallback: point.used_fallback,
        attempt_count: point.attempt_count,
        effective_model: point.effective_model.clone(),
    }
}

fn sort_rows(
    rows: &mut [ChannelStatusRow],
    field: ChannelStatusSortField,
    direction: ChannelStatusSortDirection,
) {
    rows.sort_by(|left, right| {
        let ordering = match field {
            ChannelStatusSortField::MonitorName => left
                .monitor
                .name
                .to_lowercase()
                .cmp(&right.monitor.name.to_lowercase())
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::LatestCheckedAt => left
                .selected_window
                .latest_checked_at_ms
                .cmp(&right.selected_window.latest_checked_at_ms)
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Availability => left
                .selected_window
                .effective_availability_bps
                .cmp(&right.selected_window.effective_availability_bps)
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Latency => latest_latency(left)
                .cmp(&latest_latency(right))
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Status => left
                .selected_window
                .latest_outcome
                .cmp(&right.selected_window.latest_outcome)
                .then_with(|| left.row_key.cmp(&right.row_key)),
        };
        if matches!(direction, ChannelStatusSortDirection::Desc) {
            reverse_ordering(ordering)
        } else {
            ordering
        }
    });
}

fn aggregate_rows(total_rows: u32, rows: &[ChannelStatusRow]) -> ChannelStatusAggregate {
    let mut aggregate = ChannelStatusAggregate {
        total_rows,
        returned_rows: rows.len() as u32,
        ..ChannelStatusAggregate::default()
    };
    for row in rows {
        if row.running.is_some() {
            aggregate.running_rows = aggregate.running_rows.saturating_add(1);
        }
        if row.selected_window.dirty || row.selected_window.corrupt {
            aggregate.dirty_rows = aggregate.dirty_rows.saturating_add(1);
        }
        match row.selected_window.latest_outcome {
            ChannelStatusOutcome::Available => aggregate.available_rows += 1,
            ChannelStatusOutcome::Degraded => aggregate.degraded_rows += 1,
            ChannelStatusOutcome::Unavailable => aggregate.unavailable_rows += 1,
            ChannelStatusOutcome::Skipped => aggregate.skipped_rows += 1,
            ChannelStatusOutcome::Missing => aggregate.missing_rows += 1,
        }
    }
    aggregate
}

fn freshness(rows: &[ChannelStatusRow]) -> ChannelStatusFreshness {
    let checked = rows
        .iter()
        .flat_map(|row| row.recent.iter().filter_map(|point| point.checked_at_ms))
        .collect::<Vec<_>>();
    ChannelStatusFreshness {
        newest_result_at_ms: checked.iter().max().copied(),
        oldest_result_at_ms: checked.iter().min().copied(),
        has_dirty_rollups: rows.iter().any(|row| {
            row.hourly_buckets.iter().any(|bucket| bucket.dirty)
                || row.daily_buckets.iter().any(|bucket| bucket.dirty)
        }),
        has_corrupt_rollups: rows.iter().any(|row| {
            row.hourly_buckets.iter().any(|bucket| bucket.corrupt)
                || row.daily_buckets.iter().any(|bucket| bucket.corrupt)
        }),
        running_execution_count: rows.iter().filter(|row| row.running.is_some()).count() as u32,
    }
}

fn timezone_from_bucket_set(
    set: &crate::application::monitoring::buckets::BucketWindowSet,
) -> ChannelStatusTimezone {
    let (source, requested_id) = match &set.timezone_source {
        BucketTimezoneSource::Iana => (ChannelStatusTimezoneSource::Iana, None),
        BucketTimezoneSource::UtcFallback { requested } => {
            (ChannelStatusTimezoneSource::UtcFallback, requested.clone())
        }
    };
    ChannelStatusTimezone {
        id: set.timezone_id.clone(),
        source,
        requested_id,
    }
}

fn boundaries(
    windows: &[BucketWindow],
    kind: ChannelStatusBucketKind,
) -> Vec<ChannelStatusBucketBoundary> {
    windows
        .iter()
        .map(|window| {
            debug_assert!(matches!(
                (kind, window.kind),
                (ChannelStatusBucketKind::Hour, BucketWindowKind::Hour)
                    | (ChannelStatusBucketKind::Day, BucketWindowKind::Day)
            ));
            ChannelStatusBucketBoundary {
                kind,
                start_ms: window.start_ms,
                end_ms: window.end_ms,
                label: window.label.clone(),
            }
        })
        .collect()
}

fn bucket_counts(counts: ChannelStatusBucketCounts) -> BucketCounts {
    BucketCounts {
        available_count: counts.available,
        degraded_count: counts.degraded,
        unavailable_count: counts.unavailable,
        skipped_count: counts.skipped,
    }
}

fn bucket_outcome(bucket: &ChannelStatusBucket) -> ChannelStatusOutcome {
    match bucket.state {
        ChannelStatusBucketState::Available => ChannelStatusOutcome::Available,
        ChannelStatusBucketState::Degraded => ChannelStatusOutcome::Degraded,
        ChannelStatusBucketState::Unavailable => ChannelStatusOutcome::Unavailable,
        ChannelStatusBucketState::SkippedOnly => ChannelStatusOutcome::Skipped,
        ChannelStatusBucketState::Missing | ChannelStatusBucketState::Dirty => {
            ChannelStatusOutcome::Missing
        }
    }
}

fn row_key(monitor_id: &str, station_key_id: Option<&str>) -> String {
    format!("{}|{}", monitor_id, station_key_id.unwrap_or(""))
}

fn latest_latency(row: &ChannelStatusRow) -> Option<i64> {
    row.latest.as_ref().and_then(|latest| latest.latency_ms)
}

fn reverse_ordering(ordering: Ordering) -> Ordering {
    match ordering {
        Ordering::Less => Ordering::Greater,
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => Ordering::Less,
    }
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_search(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(120).collect())
}

fn clamp_count(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

fn overlaps(window: &BucketWindow, range: &DirtyRange) -> bool {
    range.start_ms < window.end_ms && range.end_ms > window.start_ms
}

fn min_window_start(hourly: &[BucketWindow], daily: &[BucketWindow]) -> i64 {
    hourly
        .first()
        .into_iter()
        .chain(daily.first())
        .map(|window| window.start_ms)
        .min()
        .unwrap_or_default()
}

fn max_window_end(hourly: &[BucketWindow], daily: &[BucketWindow]) -> i64 {
    hourly
        .last()
        .into_iter()
        .chain(daily.last())
        .map(|window| window.end_ms)
        .max()
        .unwrap_or_default()
}

fn select_tail<T>(items: &[T], count: usize) -> &[T] {
    if items.len() <= count {
        items
    } else {
        &items[items.len() - count..]
    }
}
