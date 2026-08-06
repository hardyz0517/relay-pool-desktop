use std::collections::BTreeMap;

use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite};

use crate::{
    models::monitoring::{
        ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptRecord,
        ChannelMonitorExecutionDetail, ChannelMonitorExecutionListInput,
        ChannelMonitorExecutionSummaryV2, ChannelMonitorTargetResultRecord,
        ChannelStatusBucketCounts, ChannelStatusEndpointPing, ChannelStatusOutcome,
        ChannelStatusRecentPoint, ChannelStatusRunningExecution, ChannelStatusWorkspaceInput,
    },
    persistence::{error::PersistenceError, ReadSession},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringStatusQueryRepository;

impl MonitoringStatusQueryRepository {
    pub(crate) async fn list_execution_summaries(
        &self,
        read: &mut ReadSession,
        input: &ChannelMonitorExecutionListInput,
        limit_with_probe: u32,
    ) -> Result<Vec<ChannelMonitorExecutionSummaryV2>, PersistenceError> {
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
        query.push_bind(i64::from(limit_with_probe));

        let rows = query.build().fetch_all(read.connection()).await?;
        Ok(rows.into_iter().map(execution_summary_from_row).collect())
    }

    pub(crate) async fn execution_detail(
        &self,
        read: &mut ReadSession,
        execution_id: &str,
    ) -> Result<ChannelMonitorExecutionDetail, PersistenceError> {
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
        .bind(execution_id)
        .fetch_one(read.connection())
        .await?;
        let target_rows = sqlx::query(
            r#"
            SELECT id, execution_id, monitor_id, station_id, station_key_id,
                   terminal_outcome, terminal_failure_kind, terminal_reason,
                   requested_model, effective_model, used_fallback, attempt_count,
                   decisive_attempt_id, protocol_kind, resolved_adapter_kind,
                   resolved_dialect, client_profile_id, client_profile_version,
                   request_profile_hash, traffic_equivalence, latency_ms,
                   semantic_confidence, started_at_ms, finished_at_ms
            FROM channel_monitor_target_results
            WHERE execution_id = ?1
            ORDER BY station_key_id ASC, id ASC
            "#,
        )
        .bind(execution_id)
        .fetch_all(read.connection())
        .await?;
        Ok(ChannelMonitorExecutionDetail {
            execution: execution_summary_from_row(execution_row),
            targets: target_rows
                .into_iter()
                .map(target_result_from_row)
                .collect(),
        })
    }

    pub(crate) async fn attempt_history(
        &self,
        read: &mut ReadSession,
        input: &ChannelMonitorAttemptHistoryInput,
        limit_with_probe: u32,
    ) -> Result<Vec<ChannelMonitorAttemptRecord>, PersistenceError> {
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
        query.push_bind(i64::from(limit_with_probe));

        let rows = query.build().fetch_all(read.connection()).await?;
        Ok(rows.into_iter().map(attempt_from_row).collect())
    }

    pub(crate) async fn workspace_base_rows(
        &self,
        read: &mut ReadSession,
        input: &ChannelStatusWorkspaceInput,
        max_rows: i64,
    ) -> Result<Vec<BaseStatusRow>, PersistenceError> {
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
                COALESCE(gb.group_name, sk.group_name) AS group_name,
                COALESCE(gb.group_category_override, gb.inferred_group_category) AS effective_group_category,
                eh.status AS endpoint_ping_status,
                eh.latency_ms AS endpoint_ping_latency_ms,
                eh.checked_at AS endpoint_ping_checked_at,
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
            LEFT JOIN station_group_bindings gb ON gb.id = sk.group_binding_id
            LEFT JOIN station_endpoint_health eh
              ON eh.station_id = s.id
             AND eh.endpoint_revision = s.endpoint_revision
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
        if let Some(client_profile_id) =
            normalized_filter(input.filter.client_profile_id.as_deref())
        {
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
        query.push_bind(max_rows);

        let rows = query.build().fetch_all(read.connection()).await?;
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
                    group_name: row.get("group_name"),
                    effective_group_category: row.get("effective_group_category"),
                    endpoint_ping: endpoint_ping_from_row(&row),
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

    pub(crate) async fn workspace_recent_results(
        &self,
        read: &mut ReadSession,
        row_keys: &[StatusRowKey],
        limit: i64,
    ) -> Result<BTreeMap<StatusRowKey, Vec<ChannelStatusRecentPoint>>, PersistenceError> {
        if row_keys.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut grouped = BTreeMap::<StatusRowKey, Vec<ChannelStatusRecentPoint>>::new();
        for (monitor_id, station_key_id) in row_keys {
            let rows = if let Some(station_key_id) = station_key_id {
                sqlx::query(
                    r#"
                    SELECT
                        tr.id,
                        tr.execution_id,
                        tr.monitor_id,
                        tr.station_key_id,
                        tr.terminal_outcome,
                        tr.terminal_failure_kind,
                        tr.terminal_reason,
                        a.http_status,
                        tr.latency_ms,
                        tr.finished_at_ms,
                        tr.used_fallback,
                        tr.semantic_confidence,
                        tr.attempt_count,
                        tr.effective_model
                    FROM channel_monitor_target_results tr
                    LEFT JOIN channel_monitor_attempts a ON a.id = tr.decisive_attempt_id
                    WHERE tr.monitor_id = ?1
                      AND tr.station_key_id = ?2
                      AND tr.finished_at_ms IS NOT NULL
                    ORDER BY tr.finished_at_ms DESC, tr.id DESC
                    LIMIT ?3
                    "#,
                )
                .bind(monitor_id)
                .bind(station_key_id)
                .bind(limit)
                .fetch_all(read.connection())
                .await?
            } else {
                sqlx::query(
                    r#"
                    SELECT
                        tr.id,
                        tr.execution_id,
                        tr.monitor_id,
                        tr.station_key_id,
                        tr.terminal_outcome,
                        tr.terminal_failure_kind,
                        tr.terminal_reason,
                        a.http_status,
                        tr.latency_ms,
                        tr.finished_at_ms,
                        tr.used_fallback,
                        tr.semantic_confidence,
                        tr.attempt_count,
                        tr.effective_model
                    FROM channel_monitor_target_results tr
                    LEFT JOIN channel_monitor_attempts a ON a.id = tr.decisive_attempt_id
                    WHERE tr.monitor_id = ?1
                      AND tr.station_key_id IS NULL
                      AND tr.finished_at_ms IS NOT NULL
                    ORDER BY tr.finished_at_ms DESC, tr.id DESC
                    LIMIT ?2
                    "#,
                )
                .bind(monitor_id)
                .bind(limit)
                .fetch_all(read.connection())
                .await?
            };

            let points = grouped
                .entry((monitor_id.clone(), station_key_id.clone()))
                .or_default();
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
                    http_status: row.get("http_status"),
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

    pub(crate) async fn workspace_running_executions(
        &self,
        read: &mut ReadSession,
        row_keys: &[StatusRowKey],
    ) -> Result<BTreeMap<StatusRowKey, ChannelStatusRunningExecution>, PersistenceError> {
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
        let rows = query.build().fetch_all(read.connection()).await?;
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

    pub(crate) async fn workspace_rollups(
        &self,
        read: &mut ReadSession,
        row_keys: &[StatusRowKey],
        bucket_kind: &str,
        range_start_ms: i64,
        range_end_ms: i64,
    ) -> Result<BTreeMap<StatusRowKey, BTreeMap<i64, RollupCell>>, PersistenceError> {
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
        query.push_bind(range_start_ms);
        query.push(" AND br.bucket_start_ms < ");
        query.push_bind(range_end_ms);
        query.push(" ORDER BY br.monitor_id ASC, br.station_key_id ASC, br.bucket_start_ms ASC");

        let rows = query.build().fetch_all(read.connection()).await?;
        let mut grouped = BTreeMap::<StatusRowKey, BTreeMap<i64, RollupCell>>::new();
        for row in rows {
            let failure_counts_json: String = row.get("failure_counts_json");
            let parsed_failure_counts =
                serde_json::from_str::<BTreeMap<String, u32>>(&failure_counts_json);
            let corrupt = parsed_failure_counts.is_err();
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
                    corrupt,
                },
            );
        }
        Ok(grouped)
    }

    pub(crate) async fn workspace_dirty_ranges(
        &self,
        read: &mut ReadSession,
        row_keys: &[StatusRowKey],
        range_start_ms: i64,
        range_end_ms: i64,
    ) -> Result<BTreeMap<StatusRowKey, Vec<DirtyRange>>, PersistenceError> {
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

        let rows = query.build().fetch_all(read.connection()).await?;
        let mut grouped = BTreeMap::<StatusRowKey, Vec<DirtyRange>>::new();
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
}

pub(crate) type StatusRowKey = (String, Option<String>);

#[derive(Debug, Clone)]
pub(crate) struct BaseStatusRow {
    pub(crate) monitor_id: String,
    pub(crate) monitor_name: String,
    pub(crate) target_type: String,
    pub(crate) station_id: String,
    pub(crate) station_name: Option<String>,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_key_name: Option<String>,
    pub(crate) group_name: Option<String>,
    pub(crate) effective_group_category: Option<String>,
    pub(crate) endpoint_ping: Option<ChannelStatusEndpointPing>,
    pub(crate) enabled: bool,
    pub(crate) protocol_kind: String,
    pub(crate) client_profile_id: String,
    pub(crate) client_profile_version: i64,
    pub(crate) primary_model: String,
    pub(crate) fallback_models: Vec<String>,
    pub(crate) interval_seconds: i64,
    pub(crate) jitter_seconds: i64,
    pub(crate) next_due_at_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct RollupCell {
    pub(crate) counts: ChannelStatusBucketCounts,
    pub(crate) failure_counts: BTreeMap<String, u32>,
    pub(crate) p50_latency_ms: Option<i64>,
    pub(crate) p95_latency_ms: Option<i64>,
    pub(crate) dirty: bool,
    pub(crate) corrupt: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct DirtyRange {
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
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

fn endpoint_ping_from_row(row: &SqliteRow) -> Option<ChannelStatusEndpointPing> {
    let status = row.get::<Option<String>, _>("endpoint_ping_status")?;
    let checked_at_ms = row
        .get::<Option<String>, _>("endpoint_ping_checked_at")
        .and_then(|value| value.parse::<i64>().ok());
    Some(ChannelStatusEndpointPing {
        status,
        latency_ms: row.get("endpoint_ping_latency_ms"),
        checked_at_ms,
    })
}

fn push_scoped_values<'a>(query: &mut QueryBuilder<'a, Sqlite>, row_keys: &'a [StatusRowKey]) {
    query.push_values(row_keys, |mut row, (monitor_id, station_key_id)| {
        row.push_bind(monitor_id);
        row.push_bind(station_key_id);
    });
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalized_search(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| value.chars().count() >= 2)
        .map(str::to_string)
}

fn clamp_count(value: i64) -> u32 {
    value.clamp(0, i64::from(u32::MAX)) as u32
}
