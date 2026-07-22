use sqlx::{Row, SqliteConnection};

use crate::{
    models::{pricing::RequestCostEstimate, proxy::RequestLog, routing::StationKeyHealth},
    persistence::read_session::ReadSession,
};

use super::super::{error::PersistenceError, write_session::WriteSession};
use super::request_log_write::{
    AttemptHealthUpdate, AttemptTerminalWrite, RequestStartWrite, RequestTerminalWrite,
};

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RequestLogStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestStartPersistenceResult {
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptPersistenceResult {
    pub inserted: bool,
    pub health_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestTerminalPersistenceResult {
    pub finalized: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletedMonitorRequestWrite {
    pub(crate) request_id: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) stream: bool,
    pub(crate) status: String,
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) upstream_base_url: String,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) first_token_ms: Option<i64>,
    pub(crate) pricing: RequestCostEstimate,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) normalization_status: Option<String>,
}

impl RequestLogStore {
    pub(crate) async fn list_recent(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<RequestLog>, PersistenceError> {
        let limit = i64::from(limit);
        let rows = sqlx::query_as!(
            RequestLog,
            r#"
            SELECT id AS "id!", request_id AS "request_id?", started_at,
                   finished_at AS "finished_at?", duration_ms AS "duration_ms?",
                   method, path, model AS "model?", stream AS "stream!: bool", status,
                   lifecycle_status AS "lifecycle_status?",
                   station_key_id AS "station_key_id?", station_id AS "station_id?",
                   upstream_base_url AS "upstream_base_url?", fallback_count,
                   error_message AS "error_message?", route_policy AS "route_policy?",
                   route_reason AS "route_reason?",
                   rejected_candidates_json AS "rejected_candidates_json?",
                   body_bytes AS "body_bytes?", attempt_count AS "attempt_count?",
                   route_wait_ms AS "route_wait_ms?",
                   upstream_headers_ms AS "upstream_headers_ms?",
                   failure_source AS "failure_source?", attempts_json AS "attempts_json?",
                   completion_source AS "completion_source?",
                   prompt_tokens AS "prompt_tokens?",
                   completion_tokens AS "completion_tokens?", total_tokens AS "total_tokens?",
                   cache_creation_tokens AS "cache_creation_tokens?",
                   cache_read_tokens AS "cache_read_tokens?",
                   reasoning_effort AS "reasoning_effort?",
                   first_token_ms AS "first_token_ms?", billing_mode AS "billing_mode?",
                   estimated_input_cost AS "estimated_input_cost?",
                   estimated_output_cost AS "estimated_output_cost?",
                   estimated_total_cost AS "estimated_total_cost?",
                   base_input_cost AS "base_input_cost?",
                   base_output_cost AS "base_output_cost?",
                   base_fixed_cost AS "base_fixed_cost?",
                   base_total_cost AS "base_total_cost?", cost_currency AS "cost_currency?",
                   pricing_rule_id AS "pricing_rule_id?", pricing_source AS "pricing_source?",
                   cost_status AS "cost_status?", group_binding_id AS "group_binding_id?",
                   normalization_status AS "normalization_status?",
                   balance_scope AS "balance_scope?",
                   economic_context_json AS "economic_context_json?", created_at
            FROM request_logs
            ORDER BY created_at DESC, id DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn clear(&self, write: &mut WriteSession) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM request_logs")
            .execute(write.connection())
            .await?;
        Ok(())
    }

    pub(crate) async fn insert_completed_monitor_observation(
        &self,
        write: &mut WriteSession,
        record: &CompletedMonitorRequestWrite,
        created_at: &str,
    ) -> Result<(), PersistenceError> {
        let cost = &record.pricing;

        sqlx::query(
            r#"
            INSERT INTO request_logs (
                id, request_id, started_at, finished_at, duration_ms, method, path,
                endpoint, model, stream, status, lifecycle_status, station_key_id,
                station_id, upstream_base_url, fallback_count,
                route_policy, route_reason, completion_source, prompt_tokens,
                completion_tokens, total_tokens, cache_creation_tokens,
                cache_read_tokens, reasoning_effort, first_token_ms, billing_mode,
                estimated_input_cost, estimated_output_cost, estimated_total_cost,
                base_input_cost, base_output_cost, base_fixed_cost, base_total_cost,
                cost_currency, pricing_rule_id, pricing_source, cost_status,
                group_binding_id, normalization_status, created_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'completed',
                ?12, ?13, ?14, 0, 'channel_monitor', 'monitor_probe', 'channel_monitor',
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36
            )
            "#,
        )
        .bind(&record.request_id)
        .bind(&record.request_id)
        .bind(&record.started_at)
        .bind(record.finished_at.as_deref())
        .bind(record.duration_ms)
        .bind(&record.method)
        .bind(&record.path)
        .bind(&record.endpoint)
        .bind(&record.model)
        .bind(i64::from(record.stream as u8))
        .bind(&record.status)
        .bind(&record.station_key_id)
        .bind(&record.station_id)
        .bind(&record.upstream_base_url)
        .bind(cost.prompt_tokens)
        .bind(cost.completion_tokens)
        .bind(cost.total_tokens)
        .bind(cost.cache_creation_tokens)
        .bind(cost.cache_read_tokens)
        .bind(record.reasoning_effort.as_deref())
        .bind(record.first_token_ms)
        .bind(cost.billing_mode.as_deref())
        .bind(cost.estimated_input_cost)
        .bind(cost.estimated_output_cost)
        .bind(cost.estimated_total_cost)
        .bind(cost.base_input_cost)
        .bind(cost.base_output_cost)
        .bind(cost.base_fixed_cost)
        .bind(cost.base_total_cost)
        .bind(cost.cost_currency.as_deref())
        .bind(cost.pricing_rule_id.as_deref())
        .bind(cost.pricing_source.as_deref())
        .bind(&cost.cost_status)
        .bind(record.group_binding_id.as_deref())
        .bind(record.normalization_status.as_deref())
        .bind(created_at)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn start_request(
        &self,
        session: &mut WriteSession,
        record: &RequestStartWrite,
        created_at_ms: i64,
    ) -> Result<RequestStartPersistenceResult, PersistenceError> {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO request_logs (
                id, request_id, started_at, method, path, model, stream, status,
                lifecycle_status, endpoint, fallback_count, created_at
             ) VALUES (?, ?, ?, ?, ?, NULL, 0, 'in_progress', 'admitted', ?, 0, ?)",
        )
        .bind(&record.request_id)
        .bind(&record.request_id)
        .bind(record.received_at_ms.to_string())
        .bind(&record.method)
        .bind(&record.local_path)
        .bind(&record.endpoint)
        .bind(created_at_ms.to_string())
        .execute(session.connection())
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing =
                request_log_start_by_request_id(session.connection(), &record.request_id)
                    .await?
                    .ok_or_else(|| {
                        PersistenceError::InvariantViolation(
                            "request start duplicate missing row".to_string(),
                        )
                    })?;
            if existing != *record {
                return Err(PersistenceError::InvariantViolation(
                    "duplicate request start does not match canonical record".to_string(),
                ));
            }
        }
        Ok(RequestStartPersistenceResult {
            inserted: inserted > 0,
        })
    }

    pub(crate) async fn finish_attempt(
        &self,
        session: &mut WriteSession,
        record: &AttemptTerminalWrite,
    ) -> Result<AttemptPersistenceResult, PersistenceError> {
        if let Some(existing) = request_attempt_by_request_and_ordinal(
            session.connection(),
            &record.request_id,
            record.ordinal,
        )
        .await?
        {
            if !existing.matches(record) {
                return Err(PersistenceError::InvariantViolation(
                    "duplicate attempt terminal does not match canonical record".to_string(),
                ));
            }
            return Ok(AttemptPersistenceResult {
                inserted: false,
                health_applied: false,
            });
        }

        sqlx::query(
            "INSERT INTO request_attempts (
                request_id, ordinal, station_id, station_key_id, endpoint_revision,
                started_at_ms, terminal_kind, failure_kind, failure_blame,
                retry_disposition, health_effect, health_cooldown_until_ms,
                public_code, sanitized_detail, output_committed, terminal_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.request_id)
        .bind(i64::from(record.ordinal))
        .bind(&record.station_id)
        .bind(&record.station_key_id)
        .bind(record.endpoint_revision)
        .bind(record.started_at_ms)
        .bind(&record.terminal_kind)
        .bind(&record.failure_kind)
        .bind(&record.failure_blame)
        .bind(&record.retry_disposition)
        .bind(&record.health_effect)
        .bind(record.health_cooldown_until_ms)
        .bind(&record.public_code)
        .bind(&record.sanitized_detail)
        .bind(i64::from(record.output_committed as u8))
        .bind(record.terminal_at_ms)
        .execute(session.connection())
        .await?;

        let health_applied = apply_attempt_health(session.connection(), record).await?;
        Ok(AttemptPersistenceResult {
            inserted: true,
            health_applied,
        })
    }

    pub(crate) async fn finish_request(
        &self,
        session: &mut WriteSession,
        record: &RequestTerminalWrite,
    ) -> Result<RequestTerminalPersistenceResult, PersistenceError> {
        let finalized = update_request_terminal(session.connection(), record).await?;
        Ok(RequestTerminalPersistenceResult { finalized })
    }
}

async fn update_request_terminal(
    connection: &mut SqliteConnection,
    record: &RequestTerminalWrite,
) -> Result<bool, PersistenceError> {
    let finished_at = record.terminal_at_ms.to_string();
    let duration_ms = (record.terminal_at_ms - record.received_at_ms).max(0);
    let selected_attempt_ordinal = record.selected_attempt_ordinal.map(i64::from);
    let stream = i64::from(record.annotations.stream as u8);
    let protocol_completed = i64::from(record.protocol_completed as i32);
    let attempt_count = i64::from(record.attempt_count);
    let fallback_count = i64::from(record.fallback_count);

    let updated = sqlx::query(
        "UPDATE request_logs SET
            model = ?, stream = ?, station_key_id = ?, station_id = ?, upstream_base_url = ?,
            route_policy = ?, route_reason = ?, rejected_candidates_json = ?, body_bytes = ?,
            route_wait_ms = ?, upstream_headers_ms = ?, failure_source = ?, attempts_json = ?,
            completion_source = ?, prompt_tokens = ?, completion_tokens = ?, total_tokens = ?,
            cache_creation_tokens = ?, cache_read_tokens = ?, reasoning_effort = ?,
            first_token_ms = ?, finished_at = ?, duration_ms = ?, status = ?,
            lifecycle_status = ?, terminal_kind = ?, terminal_code = ?, terminal_detail = ?,
            protocol_completed = ?, delivery_terminal = ?, selected_attempt_ordinal = ?,
            attempt_count = ?, fallback_count = ?, terminal_at_ms = ?
         WHERE request_id = ? AND terminal_at_ms IS NULL",
    )
    .bind(record.annotations.model.as_deref())
    .bind(stream)
    .bind(record.annotations.selected_station_key_id.as_deref())
    .bind(record.annotations.selected_station_id.as_deref())
    .bind(record.annotations.upstream_base_url.as_deref())
    .bind(record.annotations.route_policy.as_deref())
    .bind(record.annotations.route_reason.as_deref())
    .bind(record.annotations.rejected_candidates_json.as_deref())
    .bind(record.annotations.body_bytes)
    .bind(record.annotations.route_wait_ms)
    .bind(record.annotations.upstream_headers_ms)
    .bind(record.annotations.failure_source.as_deref())
    .bind(record.annotations.attempts_json.as_deref())
    .bind(record.annotations.completion_source.as_deref())
    .bind(record.annotations.prompt_tokens)
    .bind(record.annotations.completion_tokens)
    .bind(record.annotations.total_tokens)
    .bind(record.annotations.cache_creation_tokens)
    .bind(record.annotations.cache_read_tokens)
    .bind(record.annotations.reasoning_effort.as_deref())
    .bind(record.annotations.first_token_ms)
    .bind(&finished_at)
    .bind(duration_ms)
    .bind(&record.status)
    .bind(&record.lifecycle_status)
    .bind(&record.terminal_kind)
    .bind(record.terminal_code.as_deref())
    .bind(record.terminal_detail.as_deref())
    .bind(protocol_completed)
    .bind(&record.delivery_terminal)
    .bind(selected_attempt_ordinal)
    .bind(attempt_count)
    .bind(fallback_count)
    .bind(record.terminal_at_ms)
    .bind(&record.request_id)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated > 0 {
        return Ok(true);
    }

    let Some(existing) = request_terminal_by_request_id(connection, &record.request_id).await?
    else {
        return Err(PersistenceError::InvariantViolation(
            "request terminal missing after update conflict".to_string(),
        ));
    };
    if !existing.matches(record) {
        return Err(PersistenceError::InvariantViolation(
            "duplicate request terminal does not match canonical record".to_string(),
        ));
    }
    Ok(false)
}

async fn request_terminal_by_request_id(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<Option<RequestTerminalRow>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_id, status, lifecycle_status, terminal_kind, terminal_code,
                    terminal_detail, protocol_completed, delivery_terminal,
                    selected_attempt_ordinal, attempt_count, fallback_count, terminal_at_ms
             FROM request_logs WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| RequestTerminalRow {
        request_id: row.get(0),
        status: row.get(1),
        lifecycle_status: row.get(2),
        terminal_kind: row.get(3),
        terminal_code: row.get(4),
        terminal_detail: row.get(5),
        protocol_completed: row.get(6),
        delivery_terminal: row.get(7),
        selected_attempt_ordinal: row.get(8),
        attempt_count: row.get(9),
        fallback_count: row.get(10),
        terminal_at_ms: row.get(11),
    }))
}

#[derive(Debug, Clone)]
struct RequestTerminalRow {
    request_id: String,
    status: String,
    lifecycle_status: Option<String>,
    terminal_kind: Option<String>,
    terminal_code: Option<String>,
    terminal_detail: Option<String>,
    protocol_completed: Option<i64>,
    delivery_terminal: Option<String>,
    selected_attempt_ordinal: Option<i64>,
    attempt_count: Option<i64>,
    fallback_count: i64,
    terminal_at_ms: Option<i64>,
}

impl RequestTerminalRow {
    fn matches(&self, record: &RequestTerminalWrite) -> bool {
        self.request_id == record.request_id
            && self.status == record.status
            && self.lifecycle_status.as_deref() == Some(record.lifecycle_status.as_str())
            && self.terminal_kind.as_deref() == Some(record.terminal_kind.as_str())
            && self.terminal_code == record.terminal_code
            && self.terminal_detail == record.terminal_detail
            && self.protocol_completed == Some(i64::from(record.protocol_completed as i32))
            && self.delivery_terminal.as_deref() == Some(record.delivery_terminal.as_str())
            && self.selected_attempt_ordinal == record.selected_attempt_ordinal.map(i64::from)
            && self.attempt_count == Some(i64::from(record.attempt_count))
            && self.fallback_count == i64::from(record.fallback_count)
            && self.terminal_at_ms.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestStartRow {
    request_id: String,
    method: String,
    local_path: String,
    endpoint: String,
    received_at_ms: i64,
}

impl PartialEq<RequestStartWrite> for RequestStartRow {
    fn eq(&self, other: &RequestStartWrite) -> bool {
        self.request_id == other.request_id
            && self.method == other.method
            && self.local_path == other.local_path
            && self.endpoint == other.endpoint
            && self.received_at_ms == other.received_at_ms
    }
}

async fn request_log_start_by_request_id(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<Option<RequestStartWrite>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_id, method, path, endpoint, CAST(started_at AS INTEGER)
             FROM request_logs WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(|row| RequestStartRow {
        request_id: row.get(0),
        method: row.get(1),
        local_path: row.get(2),
        endpoint: row.get(3),
        received_at_ms: row.get(4),
    });
    Ok(row.map(|row| RequestStartWrite {
        request_id: row.request_id,
        method: row.method,
        local_path: row.local_path,
        endpoint: row.endpoint,
        received_at_ms: row.received_at_ms,
    }))
}

struct AttemptRow {
    request_id: String,
    ordinal: i64,
    station_id: String,
    station_key_id: String,
    endpoint_revision: i64,
    started_at_ms: i64,
    terminal_kind: String,
    failure_kind: Option<String>,
    failure_blame: Option<String>,
    retry_disposition: Option<String>,
    health_effect: String,
    health_cooldown_until_ms: Option<i64>,
    public_code: Option<String>,
    sanitized_detail: Option<String>,
    output_committed: i64,
    terminal_at_ms: i64,
}

impl AttemptRow {
    fn matches(&self, record: &AttemptTerminalWrite) -> bool {
        self.request_id == record.request_id
            && self.ordinal == i64::from(record.ordinal)
            && self.station_id == record.station_id
            && self.station_key_id == record.station_key_id
            && self.endpoint_revision == record.endpoint_revision
            && self.started_at_ms == record.started_at_ms
            && self.terminal_kind == record.terminal_kind
            && self.failure_kind == record.failure_kind
            && self.failure_blame == record.failure_blame
            && self.retry_disposition == record.retry_disposition
            && self.health_effect == record.health_effect
            && self.health_cooldown_until_ms == record.health_cooldown_until_ms
            && self.public_code == record.public_code
            && self.sanitized_detail == record.sanitized_detail
            && self.output_committed == i64::from(record.output_committed as u8)
            && self.terminal_at_ms == record.terminal_at_ms
    }
}

async fn request_attempt_by_request_and_ordinal(
    connection: &mut SqliteConnection,
    request_id: &str,
    ordinal: u16,
) -> Result<Option<AttemptRow>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_id, ordinal, station_id, station_key_id, endpoint_revision,
                    started_at_ms, terminal_kind, failure_kind, failure_blame,
                    retry_disposition, health_effect, health_cooldown_until_ms,
                    public_code, sanitized_detail, output_committed, terminal_at_ms
             FROM request_attempts WHERE request_id = ? AND ordinal = ?",
    )
    .bind(request_id)
    .bind(i64::from(ordinal))
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| AttemptRow {
        request_id: row.get(0),
        ordinal: row.get(1),
        station_id: row.get(2),
        station_key_id: row.get(3),
        endpoint_revision: row.get(4),
        started_at_ms: row.get(5),
        terminal_kind: row.get(6),
        failure_kind: row.get(7),
        failure_blame: row.get(8),
        retry_disposition: row.get(9),
        health_effect: row.get(10),
        health_cooldown_until_ms: row.get(11),
        public_code: row.get(12),
        sanitized_detail: row.get(13),
        output_committed: row.get(14),
        terminal_at_ms: row.get(15),
    }))
}

async fn apply_attempt_health(
    connection: &mut SqliteConnection,
    record: &AttemptTerminalWrite,
) -> Result<bool, PersistenceError> {
    let health = match record.health_update {
        AttemptHealthUpdate::Success => Some(("success", None)),
        AttemptHealthUpdate::ObserveFailure => Some(("observe", None)),
        AttemptHealthUpdate::Cooldown { retry_after_ms } => Some(("cooldown", retry_after_ms)),
        AttemptHealthUpdate::HardFail => Some(("hard_fail", Some(15 * 60 * 1000))),
        AttemptHealthUpdate::Neutral => None,
    };

    let Some((mode, retry_after_ms)) = health else {
        return Ok(false);
    };
    if mode == "neutral" {
        return Ok(false);
    }

    let now_ms = record.terminal_at_ms;
    let now = now_ms.to_string();
    let current = station_key_health_by_key_id(connection, &record.station_key_id, &now).await?;
    let endpoint_revision = record.endpoint_revision;
    let next = match mode {
        "success" => StationKeyHealthMutation::success(
            current,
            endpoint_revision,
            &now,
            record.terminal_at_ms,
        ),
        "observe" => StationKeyHealthMutation::observe(
            current,
            endpoint_revision,
            &now,
            record.terminal_at_ms,
        ),
        "cooldown" => {
            StationKeyHealthMutation::cooldown(current, endpoint_revision, &now, retry_after_ms)
        }
        "hard_fail" => StationKeyHealthMutation::hard_fail(current, endpoint_revision, &now),
        _ => StationKeyHealthMutation::neutral(current, endpoint_revision, &now),
    };
    upsert_station_key_health(connection, &record.station_key_id, next).await?;
    Ok(true)
}

struct StationKeyHealthMutation {
    endpoint_revision: i64,
    last_success_at: Option<String>,
    last_failure_at: Option<String>,
    consecutive_failures: i64,
    success_count: i64,
    failure_count: i64,
    total_duration_ms: i64,
    avg_latency_ms: Option<i64>,
    last_error_summary: Option<String>,
    cooldown_until: Option<String>,
    updated_at: String,
}

impl StationKeyHealthMutation {
    fn success(
        current: crate::models::routing::StationKeyHealth,
        endpoint_revision: i64,
        now: &str,
        duration_ms: i64,
    ) -> Self {
        let success_count = current.success_count + 1;
        let total_duration_ms = current
            .avg_latency_ms
            .map(|avg| avg * current.success_count)
            .unwrap_or(0)
            + duration_ms.max(0);
        let avg_latency_ms = if success_count > 0 {
            Some(total_duration_ms / success_count)
        } else {
            None
        };
        Self {
            endpoint_revision,
            last_success_at: Some(now.to_string()),
            last_failure_at: current.last_failure_at,
            consecutive_failures: 0,
            success_count,
            failure_count: current.failure_count,
            total_duration_ms,
            avg_latency_ms,
            last_error_summary: None,
            cooldown_until: None,
            updated_at: now.to_string(),
        }
    }

    fn observe(
        current: crate::models::routing::StationKeyHealth,
        endpoint_revision: i64,
        now: &str,
        _duration_ms: i64,
    ) -> Self {
        let consecutive_failures = current.consecutive_failures + 1;
        let cooldown_until = if consecutive_failures >= 3 {
            Some(cooldown_until_with_threshold(consecutive_failures, 3, now))
        } else {
            None
        };
        Self {
            endpoint_revision,
            last_success_at: current.last_success_at,
            last_failure_at: Some(now.to_string()),
            consecutive_failures,
            success_count: current.success_count,
            failure_count: current.failure_count + 1,
            total_duration_ms: current
                .avg_latency_ms
                .map(|avg| avg * current.success_count)
                .unwrap_or(0),
            avg_latency_ms: current.avg_latency_ms,
            last_error_summary: current.last_error_summary,
            cooldown_until,
            updated_at: now.to_string(),
        }
    }

    fn cooldown(
        current: crate::models::routing::StationKeyHealth,
        endpoint_revision: i64,
        now: &str,
        retry_after_ms: Option<i64>,
    ) -> Self {
        let consecutive_failures = current.consecutive_failures + 1;
        let cooldown_until = retry_after_ms
            .map(|retry_after_ms| now.parse::<i64>().unwrap_or(0) + retry_after_ms.max(0));
        Self {
            endpoint_revision,
            last_success_at: current.last_success_at,
            last_failure_at: Some(now.to_string()),
            consecutive_failures,
            success_count: current.success_count,
            failure_count: current.failure_count + 1,
            total_duration_ms: current
                .avg_latency_ms
                .map(|avg| avg * current.success_count)
                .unwrap_or(0),
            avg_latency_ms: current.avg_latency_ms,
            last_error_summary: current.last_error_summary,
            cooldown_until: cooldown_until.map(|value| value.to_string()),
            updated_at: now.to_string(),
        }
    }

    fn hard_fail(
        current: crate::models::routing::StationKeyHealth,
        endpoint_revision: i64,
        now: &str,
    ) -> Self {
        let consecutive_failures = current.consecutive_failures + 1;
        Self {
            endpoint_revision,
            last_success_at: current.last_success_at,
            last_failure_at: Some(now.to_string()),
            consecutive_failures,
            success_count: current.success_count,
            failure_count: current.failure_count + 1,
            total_duration_ms: current
                .avg_latency_ms
                .map(|avg| avg * current.success_count)
                .unwrap_or(0),
            avg_latency_ms: current.avg_latency_ms,
            last_error_summary: current.last_error_summary,
            cooldown_until: Some((now.parse::<i64>().unwrap_or(0) + 15 * 60 * 1000).to_string()),
            updated_at: now.to_string(),
        }
    }

    fn neutral(
        current: crate::models::routing::StationKeyHealth,
        endpoint_revision: i64,
        now: &str,
    ) -> Self {
        Self {
            endpoint_revision,
            last_success_at: current.last_success_at,
            last_failure_at: current.last_failure_at,
            consecutive_failures: current.consecutive_failures,
            success_count: current.success_count,
            failure_count: current.failure_count,
            total_duration_ms: current
                .avg_latency_ms
                .map(|avg| avg * current.success_count)
                .unwrap_or(0),
            avg_latency_ms: current.avg_latency_ms,
            last_error_summary: current.last_error_summary,
            cooldown_until: current.cooldown_until,
            updated_at: now.to_string(),
        }
    }
}

async fn station_key_health_by_key_id(
    connection: &mut SqliteConnection,
    station_key_id: &str,
    now: &str,
) -> Result<StationKeyHealth, PersistenceError> {
    let row = sqlx::query(
        "SELECT station_key_id, last_success_at, last_failure_at, consecutive_failures,
                    success_count, failure_count, avg_latency_ms, last_error_summary,
                    cooldown_until, updated_at
             FROM station_key_health WHERE station_key_id = ?",
    )
    .bind(station_key_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(|row| StationKeyHealth {
        station_key_id: row.get(0),
        last_success_at: row.get(1),
        last_failure_at: row.get(2),
        consecutive_failures: row.get(3),
        success_count: row.get(4),
        failure_count: row.get(5),
        avg_latency_ms: row.get(6),
        last_error_summary: row.get(7),
        cooldown_until: row.get(8),
        updated_at: row.get(9),
    });
    Ok(row.unwrap_or_else(|| StationKeyHealth {
        station_key_id: station_key_id.to_string(),
        last_success_at: None,
        last_failure_at: None,
        consecutive_failures: 0,
        success_count: 0,
        failure_count: 0,
        avg_latency_ms: None,
        last_error_summary: None,
        cooldown_until: None,
        updated_at: now.to_string(),
    }))
}

async fn upsert_station_key_health(
    connection: &mut SqliteConnection,
    station_key_id: &str,
    mutation: StationKeyHealthMutation,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO station_key_health (
            station_key_id, endpoint_revision, last_success_at, last_failure_at, consecutive_failures,
            success_count, failure_count, total_duration_ms, avg_latency_ms,
            last_error_summary, cooldown_until, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(station_key_id) DO UPDATE SET
            endpoint_revision = excluded.endpoint_revision,
            last_success_at = excluded.last_success_at,
            last_failure_at = excluded.last_failure_at,
            consecutive_failures = excluded.consecutive_failures,
            success_count = excluded.success_count,
            failure_count = excluded.failure_count,
            total_duration_ms = excluded.total_duration_ms,
            avg_latency_ms = excluded.avg_latency_ms,
            last_error_summary = excluded.last_error_summary,
            cooldown_until = excluded.cooldown_until,
            updated_at = excluded.updated_at",
    )
    .bind(station_key_id)
    .bind(mutation.endpoint_revision)
    .bind(mutation.last_success_at)
    .bind(mutation.last_failure_at)
    .bind(mutation.consecutive_failures)
    .bind(mutation.success_count)
    .bind(mutation.failure_count)
    .bind(mutation.total_duration_ms)
    .bind(mutation.avg_latency_ms)
    .bind(mutation.last_error_summary)
    .bind(mutation.cooldown_until)
    .bind(mutation.updated_at)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn cooldown_until_with_threshold(
    consecutive_failures: i64,
    consecutive_failure_threshold: i64,
    now: &str,
) -> String {
    let now = now.parse::<i64>().unwrap_or(0);
    let threshold = consecutive_failure_threshold.max(1);
    let duration_ms = match consecutive_failures - threshold {
        failures_before_threshold if failures_before_threshold < 0 => 0,
        0 => 2 * 60 * 1000,
        1 => 5 * 60 * 1000,
        _ => 15 * 60 * 1000,
    };
    (now + duration_ms).to_string()
}

#[cfg(test)]
mod v2_tests {
    use std::collections::BTreeSet;

    use semver::Version;
    use sqlx::{sqlite::SqlitePoolOptions, Row};

    use crate::persistence::{
        migrations::migrator,
        runtime::PersistenceRuntime,
        schema_compatibility::BinaryCompatibility,
        stores::request_log_write::{
            AttemptHealthUpdate, AttemptTerminalWrite, RequestLogAnnotationsWrite,
            RequestStartWrite, RequestTerminalWrite,
        },
    };

    use super::RequestLogStore;

    fn binary() -> BinaryCompatibility {
        BinaryCompatibility {
            app_version: Version::new(0, 3, 1),
            database_generation: 2,
            readable_schema: 1..=8,
            writable_schema: BTreeSet::from([8]),
        }
    }

    async fn runtime() -> PersistenceRuntime {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool.sqlite3");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("pool");
        migrator().run(&pool).await.expect("migrations");
        pool.close().await;
        // Keep the directory alive for the lifetime of the test runtime.
        std::mem::forget(root);
        PersistenceRuntime::open(&path, binary())
            .await
            .expect("runtime")
    }

    fn start_record(id: &str) -> RequestStartWrite {
        RequestStartWrite {
            request_id: id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/chat/completions".to_string(),
            endpoint: "chat_completions".to_string(),
            received_at_ms: 1000,
        }
    }

    fn attempt_record(id: &str) -> AttemptTerminalWrite {
        AttemptTerminalWrite {
            request_id: id.to_string(),
            ordinal: 0,
            station_id: "station-1".to_string(),
            station_key_id: "key-1".to_string(),
            endpoint_revision: 1,
            started_at_ms: 1001,
            terminal_kind: "succeeded".to_string(),
            failure_kind: None,
            failure_blame: None,
            retry_disposition: None,
            health_effect: "success".to_string(),
            health_cooldown_until_ms: None,
            health_update: AttemptHealthUpdate::Success,
            public_code: None,
            sanitized_detail: None,
            output_committed: true,
            terminal_at_ms: 1100,
        }
    }

    fn terminal_record(id: &str) -> RequestTerminalWrite {
        RequestTerminalWrite {
            request_id: id.to_string(),
            received_at_ms: 1000,
            status: "success".to_string(),
            lifecycle_status: "completed".to_string(),
            terminal_kind: "completed".to_string(),
            terminal_code: Some("request_completed".to_string()),
            terminal_detail: None,
            protocol_completed: true,
            delivery_terminal: "BodyCompleted".to_string(),
            selected_attempt_ordinal: Some(0),
            attempt_count: 1,
            fallback_count: 0,
            terminal_at_ms: 1100,
            annotations: RequestLogAnnotationsWrite {
                model: Some("gpt-test".to_string()),
                stream: true,
                selected_station_key_id: Some("key-1".to_string()),
                selected_station_id: Some("station-1".to_string()),
                upstream_base_url: Some("https://station.test/v1".to_string()),
                route_policy: Some("stable_first".to_string()),
                route_reason: Some("healthy key".to_string()),
                rejected_candidates_json: Some("[]".to_string()),
                body_bytes: Some(128),
                route_wait_ms: Some(3),
                upstream_headers_ms: Some(7),
                failure_source: Some("upstream".to_string()),
                attempts_json: Some("[]".to_string()),
                completion_source: Some("chat.completion".to_string()),
                prompt_tokens: Some(11),
                completion_tokens: Some(13),
                total_tokens: Some(24),
                cache_creation_tokens: Some(2),
                cache_read_tokens: Some(5),
                reasoning_effort: Some("high".to_string()),
                first_token_ms: Some(17),
            },
        }
    }

    #[tokio::test]
    async fn request_start_is_idempotent() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        let record = start_record("req-start");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(
            store
                .start_request(&mut first, &record, 1000)
                .await
                .expect("start")
                .inserted
        );
        first.commit().await.expect("commit");
        let mut second = runtime.begin_write().await.expect("write");
        assert!(
            !store
                .start_request(&mut second, &record, 1000)
                .await
                .expect("duplicate")
                .inserted
        );
        second.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn recent_request_logs_can_be_listed_and_cleared() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        let record = start_record("req-list");
        let mut write = runtime.begin_write().await.expect("write");
        store
            .start_request(&mut write, &record, 1000)
            .await
            .expect("start");
        write.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read");
        let logs = store.list_recent(&mut read, 500).await.expect("list");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].request_id.as_deref(), Some("req-list"));
        drop(read);

        let mut write = runtime.begin_write().await.expect("write");
        store.clear(&mut write).await.expect("clear");
        write.commit().await.expect("commit");
        let mut read = runtime.begin_read().await.expect("read");
        assert!(store
            .list_recent(&mut read, 500)
            .await
            .expect("empty list")
            .is_empty());
    }

    #[tokio::test]
    async fn attempt_and_health_are_applied_once() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        seed_attempt_owner(&runtime).await;
        let record = start_record("req-attempt");
        let mut session = runtime.begin_write().await.expect("write");
        store
            .start_request(&mut session, &record, 1000)
            .await
            .expect("start");
        session.commit().await.expect("commit");
        let attempt = attempt_record("req-attempt");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(
            store
                .finish_attempt(&mut first, &attempt)
                .await
                .expect("attempt")
                .inserted
        );
        first.commit().await.expect("commit");
        let mut duplicate = runtime.begin_write().await.expect("write");
        assert!(
            !store
                .finish_attempt(&mut duplicate, &attempt)
                .await
                .expect("duplicate")
                .inserted
        );
        duplicate.commit().await.expect("commit");
        let mut read = runtime.begin_read().await.expect("read");
        let row = sqlx::query("SELECT COUNT(*), success_count FROM request_attempts a JOIN station_key_health h ON h.station_key_id = a.station_key_id WHERE a.request_id = ?")
            .bind("req-attempt")
            .fetch_one(read.connection())
            .await
            .expect("health row");
        assert_eq!(row.get::<i64, _>(0), 1);
        assert_eq!(row.get::<i64, _>(1), 1);
    }

    async fn seed_attempt_owner(runtime: &PersistenceRuntime) {
        let mut session = runtime.begin_write().await.expect("seed write");
        sqlx::query(
            "INSERT INTO stations (id, name, station_type, website_url, api_base_url, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("station-1")
        .bind("Test station")
        .bind("openai-compatible")
        .bind("https://station.test")
        .bind("https://station.test/v1")
        .bind("0")
        .bind("0")
        .execute(session.connection())
        .await
        .expect("seed station");
        sqlx::query("INSERT INTO station_keys (id, station_id) VALUES (?, ?)")
            .bind("key-1")
            .bind("station-1")
            .execute(session.connection())
            .await
            .expect("seed key");
        session.commit().await.expect("seed commit");
    }

    #[tokio::test]
    async fn request_terminal_uses_compare_and_set() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        seed_attempt_owner(&runtime).await;
        let record = start_record("req-terminal");
        let mut start = runtime.begin_write().await.expect("write");
        store
            .start_request(&mut start, &record, 1000)
            .await
            .expect("start");
        start.commit().await.expect("commit");
        let final_record = terminal_record("req-terminal");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(
            store
                .finish_request(&mut first, &final_record)
                .await
                .expect("finish")
                .finalized
        );
        first.commit().await.expect("commit");
        let mut duplicate = runtime.begin_write().await.expect("write");
        assert!(
            !store
                .finish_request(&mut duplicate, &final_record)
                .await
                .expect("duplicate")
                .finalized
        );
        duplicate.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read");
        let row = sqlx::query(
            "SELECT model, stream, station_key_id, station_id, upstream_base_url,
                    route_policy, route_reason, rejected_candidates_json, body_bytes,
                    route_wait_ms, upstream_headers_ms, failure_source, attempts_json,
                    completion_source, prompt_tokens, completion_tokens, total_tokens,
                    cache_creation_tokens, cache_read_tokens, reasoning_effort,
                    first_token_ms, finished_at, duration_ms, status, lifecycle_status,
                    terminal_kind, terminal_code, protocol_completed, delivery_terminal,
                    selected_attempt_ordinal, attempt_count, fallback_count, terminal_at_ms
             FROM request_logs WHERE request_id = ?",
        )
        .bind("req-terminal")
        .fetch_one(read.connection())
        .await
        .expect("terminal row");
        assert_eq!(row.get::<Option<String>, _>(0).as_deref(), Some("gpt-test"));
        assert_eq!(row.get::<i64, _>(1), 1);
        assert_eq!(row.get::<Option<String>, _>(2).as_deref(), Some("key-1"));
        assert_eq!(
            row.get::<Option<String>, _>(3).as_deref(),
            Some("station-1")
        );
        assert_eq!(
            row.get::<Option<String>, _>(4).as_deref(),
            Some("https://station.test/v1")
        );
        assert_eq!(
            row.get::<Option<String>, _>(5).as_deref(),
            Some("stable_first")
        );
        assert_eq!(
            row.get::<Option<String>, _>(6).as_deref(),
            Some("healthy key")
        );
        assert_eq!(row.get::<Option<String>, _>(7).as_deref(), Some("[]"));
        assert_eq!(row.get::<Option<i64>, _>(8), Some(128));
        assert_eq!(row.get::<Option<i64>, _>(9), Some(3));
        assert_eq!(row.get::<Option<i64>, _>(10), Some(7));
        assert_eq!(
            row.get::<Option<String>, _>(11).as_deref(),
            Some("upstream")
        );
        assert_eq!(row.get::<Option<String>, _>(12).as_deref(), Some("[]"));
        assert_eq!(
            row.get::<Option<String>, _>(13).as_deref(),
            Some("chat.completion")
        );
        assert_eq!(row.get::<Option<i64>, _>(14), Some(11));
        assert_eq!(row.get::<Option<i64>, _>(15), Some(13));
        assert_eq!(row.get::<Option<i64>, _>(16), Some(24));
        assert_eq!(row.get::<Option<i64>, _>(17), Some(2));
        assert_eq!(row.get::<Option<i64>, _>(18), Some(5));
        assert_eq!(row.get::<Option<String>, _>(19).as_deref(), Some("high"));
        assert_eq!(row.get::<Option<i64>, _>(20), Some(17));
        assert_eq!(row.get::<Option<String>, _>(21).as_deref(), Some("1100"));
        assert_eq!(row.get::<Option<i64>, _>(22), Some(100));
        assert_eq!(row.get::<String, _>(23), "success");
        assert_eq!(
            row.get::<Option<String>, _>(24).as_deref(),
            Some("completed")
        );
        assert_eq!(
            row.get::<Option<String>, _>(25).as_deref(),
            Some("completed")
        );
        assert_eq!(
            row.get::<Option<String>, _>(26).as_deref(),
            Some("request_completed")
        );
        assert_eq!(row.get::<Option<i64>, _>(27), Some(1));
        assert_eq!(
            row.get::<Option<String>, _>(28).as_deref(),
            Some("BodyCompleted")
        );
        assert_eq!(row.get::<Option<i64>, _>(29), Some(0));
        assert_eq!(row.get::<Option<i64>, _>(30), Some(1));
        assert_eq!(row.get::<i64, _>(31), 0);
        assert_eq!(row.get::<Option<i64>, _>(32), Some(1100));
    }
}
