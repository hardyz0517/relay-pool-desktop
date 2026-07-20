use sqlx::{Row, SqliteConnection};

use crate::{
    models::routing::StationKeyHealth,
    services::proxy::lifecycle::{
        attempt::{AttemptTerminal, AttemptTerminalRecord, HealthEffect},
        request::{FinalRequestRecord, RequestStartRecord},
    },
};

use super::super::{error::PersistenceError, write_session::WriteSession};

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

impl RequestLogStore {
    pub(crate) async fn start_request(
        &self,
        session: &mut WriteSession,
        record: &RequestStartRecord,
    ) -> Result<RequestStartPersistenceResult, PersistenceError> {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO request_logs (
                id, request_id, started_at, method, path, model, stream, status,
                lifecycle_status, endpoint, fallback_count, created_at
             ) VALUES (?, ?, ?, ?, ?, NULL, 0, 'in_progress', 'admitted', ?, 0, ?)",
        )
        .bind(&record.context.request_id)
        .bind(&record.context.request_id)
        .bind(record.context.received_at_ms.to_string())
        .bind(&record.context.method)
        .bind(&record.context.local_path)
        .bind(&record.context.endpoint)
        .bind(now_string())
        .execute(session.connection())
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing = request_log_start_by_request_id(
                session.connection(),
                &record.context.request_id,
            )
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
        record: &AttemptTerminalRecord,
    ) -> Result<AttemptPersistenceResult, PersistenceError> {
        if let Some(existing) = request_attempt_by_request_and_ordinal(
            session.connection(),
            &record.context.attempt_id.request_id,
            record.context.attempt_id.ordinal,
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

        let attempt = AttemptRow::from_record(record);
        sqlx::query(
            "INSERT INTO request_attempts (
                request_id, ordinal, station_id, station_key_id, endpoint_revision,
                started_at_ms, terminal_kind, failure_kind, failure_blame,
                retry_disposition, health_effect, health_cooldown_until_ms,
                public_code, sanitized_detail, output_committed, terminal_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&attempt.request_id)
        .bind(attempt.ordinal)
        .bind(&attempt.station_id)
        .bind(&attempt.station_key_id)
        .bind(attempt.endpoint_revision)
        .bind(attempt.started_at_ms)
        .bind(&attempt.terminal_kind)
        .bind(&attempt.failure_kind)
        .bind(&attempt.failure_blame)
        .bind(&attempt.retry_disposition)
        .bind(&attempt.health_effect)
        .bind(attempt.health_cooldown_until_ms)
        .bind(&attempt.public_code)
        .bind(&attempt.sanitized_detail)
        .bind(attempt.output_committed)
        .bind(attempt.terminal_at_ms)
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
        record: &FinalRequestRecord,
    ) -> Result<RequestTerminalPersistenceResult, PersistenceError> {
        let finalized = update_request_terminal(session.connection(), record).await?;
        Ok(RequestTerminalPersistenceResult { finalized })
    }
}

async fn update_request_terminal(
    connection: &mut SqliteConnection,
    record: &FinalRequestRecord,
) -> Result<bool, PersistenceError> {
    let now_ms = crate::services::database::now_millis_for_services() as i64;
    let finished_at = now_ms.to_string();
    let duration_ms = (now_ms - record.context.received_at_ms).max(0);
    let (
        status,
        lifecycle_status,
        terminal_kind,
        terminal_code,
        terminal_detail,
        protocol_completed,
    ) = request_terminal_shape(record);
    let selected_attempt_ordinal = record
        .selected_attempt_id
        .as_ref()
        .map(|attempt_id| i64::from(attempt_id.ordinal));
    let stream = i64::from(record.annotations.stream as u8);
    let protocol_completed = i64::from(protocol_completed as i32);
    let delivery_terminal = format!("{:?}", record.terminal.delivery);
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
    .bind(status)
    .bind(lifecycle_status)
    .bind(terminal_kind)
    .bind(terminal_code.as_deref())
    .bind(terminal_detail.as_deref())
    .bind(protocol_completed)
    .bind(&delivery_terminal)
    .bind(selected_attempt_ordinal)
    .bind(attempt_count)
    .bind(fallback_count)
    .bind(now_ms)
    .bind(&record.context.request_id)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated > 0 {
        return Ok(true);
    }

    let Some(existing) =
        request_terminal_by_request_id(connection, &record.context.request_id).await?
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
    fn matches(&self, record: &FinalRequestRecord) -> bool {
        let (
            status,
            lifecycle_status,
            terminal_kind,
            terminal_code,
            terminal_detail,
            protocol_completed,
        ) = request_terminal_shape(record);
        let selected_attempt_ordinal = record
            .selected_attempt_id
            .as_ref()
            .map(|attempt_id| i64::from(attempt_id.ordinal));
        self.request_id == record.context.request_id
            && self.status == status
            && self.lifecycle_status.as_deref() == Some(lifecycle_status)
            && self.terminal_kind.as_deref() == Some(terminal_kind)
            && self.terminal_code.as_deref() == terminal_code.as_deref()
            && self.terminal_detail.as_deref() == terminal_detail.as_deref()
            && self.protocol_completed == Some(i64::from(protocol_completed as i32))
            && self.delivery_terminal.as_deref()
                == Some(format!("{:?}", record.terminal.delivery).as_str())
            && self.selected_attempt_ordinal == selected_attempt_ordinal
            && self.attempt_count == Some(i64::from(record.attempt_count))
            && self.fallback_count == i64::from(record.fallback_count)
            && self.terminal_at_ms.is_some()
    }
}

fn request_terminal_shape(
    record: &FinalRequestRecord,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<String>,
    Option<String>,
    bool,
) {
    match &record.terminal.terminal {
        crate::services::proxy::lifecycle::request::RequestTerminal::Completed(_) => (
            "success",
            "completed",
            "completed",
            Some("request_completed".to_string()),
            None,
            true,
        ),
        crate::services::proxy::lifecycle::request::RequestTerminal::PartialSuccess(_) => (
            "success",
            "partial_success",
            "partial_success",
            Some("request_partial_success".to_string()),
            None,
            true,
        ),
        crate::services::proxy::lifecycle::request::RequestTerminal::Failed(failure) => (
            "failed",
            "failed",
            "failed",
            Some(failure.code.clone()),
            failure.detail.clone(),
            false,
        ),
        crate::services::proxy::lifecycle::request::RequestTerminal::Interrupted(failure) => (
            "interrupted",
            "interrupted",
            "interrupted",
            Some(format!("{:?}", failure.terminal)),
            failure
                .detail
                .clone()
                .or_else(|| Some("downstream disconnected".to_string())),
            false,
        ),
    }
}

fn now_string() -> String {
    crate::services::database::now_millis_for_services().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestStartRow {
    request_id: String,
    method: String,
    local_path: String,
    endpoint: String,
    received_at_ms: i64,
}

impl PartialEq<RequestStartRecord> for RequestStartRow {
    fn eq(&self, other: &RequestStartRecord) -> bool {
        self.request_id == other.context.request_id
            && self.method == other.context.method
            && self.local_path == other.context.local_path
            && self.endpoint == other.context.endpoint
            && self.received_at_ms == other.context.received_at_ms
    }
}

async fn request_log_start_by_request_id(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<Option<RequestStartRecord>, PersistenceError> {
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
    Ok(row.map(|row| RequestStartRecord {
        context: crate::services::proxy::lifecycle::request::RequestContextSnapshot {
            request_id: row.request_id,
            method: row.method,
            local_path: row.local_path,
            endpoint: row.endpoint,
            received_at_ms: row.received_at_ms,
        },
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
    fn from_record(record: &AttemptTerminalRecord) -> Self {
        let (
            terminal_kind,
            failure_kind,
            failure_blame,
            retry_disposition,
            health_effect,
            public_code,
            sanitized_detail,
            health_cooldown_until_ms,
        ) = match &record.terminal {
            AttemptTerminal::Succeeded => (
                "succeeded".to_string(),
                None,
                None,
                None,
                "success".to_string(),
                None,
                None,
                None,
            ),
            AttemptTerminal::Failed(failure) => (
                "failed".to_string(),
                Some(format!("{:?}", failure.kind)),
                Some(format!("{:?}", failure.blame)),
                Some(format!("{:?}", failure.retry)),
                format!("{:?}", failure.health),
                Some(failure.public_code.clone()),
                failure.sanitized_detail.clone(),
                None,
            ),
            AttemptTerminal::Abandoned { reason } => (
                "abandoned".to_string(),
                None,
                None,
                Some("StopRequest".to_string()),
                "neutral".to_string(),
                Some(reason.clone()),
                None,
                None,
            ),
        };

        Self {
            request_id: record.context.attempt_id.request_id.clone(),
            ordinal: i64::from(record.context.attempt_id.ordinal),
            station_id: record.context.station_id.clone(),
            station_key_id: record.context.station_key_id.clone(),
            endpoint_revision: record.context.endpoint_revision,
            started_at_ms: record.context.started_at_ms,
            terminal_kind,
            failure_kind,
            failure_blame,
            retry_disposition,
            health_effect,
            health_cooldown_until_ms,
            public_code,
            sanitized_detail,
            output_committed: if record.output_committed { 1 } else { 0 },
            terminal_at_ms: record.terminal_at_ms,
        }
    }

    fn matches(&self, record: &AttemptTerminalRecord) -> bool {
        let expected = Self::from_record(record);
        self.request_id == expected.request_id
            && self.ordinal == expected.ordinal
            && self.station_id == expected.station_id
            && self.station_key_id == expected.station_key_id
            && self.endpoint_revision == expected.endpoint_revision
            && self.started_at_ms == expected.started_at_ms
            && self.terminal_kind == expected.terminal_kind
            && self.failure_kind == expected.failure_kind
            && self.failure_blame == expected.failure_blame
            && self.retry_disposition == expected.retry_disposition
            && self.health_effect == expected.health_effect
            && self.public_code == expected.public_code
            && self.sanitized_detail == expected.sanitized_detail
            && self.output_committed == expected.output_committed
            && self.terminal_at_ms == expected.terminal_at_ms
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
    record: &AttemptTerminalRecord,
) -> Result<bool, PersistenceError> {
    let health = match &record.terminal {
        AttemptTerminal::Succeeded => Some(("success", None)),
        AttemptTerminal::Failed(failure) => match failure.health {
            HealthEffect::Success => Some(("success", None)),
            HealthEffect::ObserveFailure => Some(("observe", None)),
            HealthEffect::Cooldown { retry_after_ms } => Some(("cooldown", retry_after_ms)),
            HealthEffect::HardFail => Some(("hard_fail", Some(15 * 60 * 1000))),
            HealthEffect::Neutral => None,
        },
        AttemptTerminal::Abandoned { .. } => Some(("neutral", None)),
    };

    let Some((mode, retry_after_ms)) = health else {
        return Ok(false);
    };
    if mode == "neutral" {
        return Ok(false);
    }

    let now_ms = record.terminal_at_ms;
    let now = now_ms.to_string();
    let current =
        station_key_health_by_key_id(connection, &record.context.station_key_id).await?;
    let endpoint_revision = record.context.endpoint_revision;
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
    upsert_station_key_health(connection, &record.context.station_key_id, next).await?;
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
        updated_at: now_string(),
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

#[cfg(any())]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        models::{station_keys::CreateStationKeyInput, stations::CreateStationInput},
        persistence::runtime::{PersistenceRuntime, PersistenceRuntimeConfig},
        services::{
            database::AppDatabase,
            proxy::lifecycle::{
                attempt::{AttemptContext, AttemptTerminal},
                delivery::DeliveryTerminal,
                request::{
                    AttemptId, FinalRequestRecord, RequestCompletion, RequestContextSnapshot,
                    RequestLifecycle, RequestLogAnnotations, RequestTerminal,
                },
            },
        },
    };

    use super::{RequestLogStore, RequestStartRecord};

    #[tokio::test]
    async fn request_start_is_idempotent_and_preserves_one_row() {
        let database = file_database("request-start").expect("database");
        let runtime = PersistenceRuntime::open(PersistenceRuntimeConfig::new(database.db_path()))
            .await
            .expect("runtime");
        let store = RequestLogStore;
        let record = request_start_record("req-start");

        let mut first_session = runtime.begin_write().expect("session");
        let first = store
            .start_request(&mut first_session, &record)
            .expect("first start");
        first_session.commit().expect("commit");

        let mut second_session = runtime.begin_write().expect("session");
        let second = store
            .start_request(&mut second_session, &record)
            .expect("second start");
        second_session.commit().expect("commit");

        assert!(first.inserted);
        assert!(!second.inserted);
        let rows = database
            .connection_for_repository_tests()
            .expect("connection")
            .query_row(
                "SELECT COUNT(*) FROM request_logs WHERE request_id = ?1",
                [record.context.request_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count");
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn attempt_terminal_applies_health_once() {
        let database = file_database("attempt-terminal").expect("database");
        let runtime = PersistenceRuntime::open(PersistenceRuntimeConfig::new(database.db_path()))
            .await
            .expect("runtime");
        let store = RequestLogStore;
        let request = request_start_record("req-attempt");
        let seeded = seed_station_key(&database, "attempt-terminal");
        let attempt = attempt_record(
            "req-attempt",
            &seeded.station_id,
            &seeded.station_key_id,
            seeded.endpoint_revision,
            0,
        );

        let mut start_session = runtime.begin_write().expect("session");
        store
            .start_request(&mut start_session, &request)
            .expect("start");
        start_session.commit().expect("commit");

        let mut attempt_session = runtime.begin_write().expect("session");
        let first = store
            .finish_attempt(&mut attempt_session, &attempt)
            .expect("first attempt");
        attempt_session.commit().expect("commit");

        let mut duplicate_session = runtime.begin_write().expect("session");
        let second = store
            .finish_attempt(&mut duplicate_session, &attempt)
            .expect("duplicate attempt");
        duplicate_session.commit().expect("commit");

        assert!(first.inserted);
        assert!(first.health_applied);
        assert!(!second.inserted);
        assert!(!second.health_applied);

        let health = database
            .get_station_key_health(seeded.station_key_id)
            .expect("health");
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
    }

    #[tokio::test]
    async fn request_terminal_cas_is_idempotent() {
        let database = file_database("request-terminal").expect("database");
        let runtime = PersistenceRuntime::open(PersistenceRuntimeConfig::new(database.db_path()))
            .await
            .expect("runtime");
        let store = RequestLogStore;
        let request = request_start_record("req-terminal");
        let final_record = request_terminal_record("req-terminal");

        let mut start_session = runtime.begin_write().expect("session");
        store
            .start_request(&mut start_session, &request)
            .expect("start");
        start_session.commit().expect("commit");

        let mut finish_session = runtime.begin_write().expect("session");
        let first = store
            .finish_request(&mut finish_session, &final_record)
            .expect("first finish");
        finish_session.commit().expect("commit");

        let mut duplicate_session = runtime.begin_write().expect("session");
        let second = store
            .finish_request(&mut duplicate_session, &final_record)
            .expect("duplicate finish");
        duplicate_session.commit().expect("commit");

        assert!(first.finalized);
        assert!(!second.finalized);
    }

    #[tokio::test]
    async fn request_terminal_persists_log_annotations() {
        let database = file_database("request-terminal-annotations").expect("database");
        let runtime = PersistenceRuntime::open(PersistenceRuntimeConfig::new(database.db_path()))
            .await
            .expect("runtime");
        let store = RequestLogStore;
        let request = request_start_record("req-terminal-annotations");
        let mut final_record = request_terminal_record("req-terminal-annotations");
        final_record.annotations = RequestLogAnnotations {
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
        };

        let mut start_session = runtime.begin_write().expect("session");
        store
            .start_request(&mut start_session, &request)
            .expect("start");
        start_session.commit().expect("commit");

        let mut finish_session = runtime.begin_write().expect("session");
        store
            .finish_request(&mut finish_session, &final_record)
            .expect("finish");
        finish_session.commit().expect("commit");

        let connection = database
            .connection_for_repository_tests()
            .expect("connection");
        let observed = connection
            .query_row(
                "SELECT model, stream, station_key_id, station_id, upstream_base_url,
                        route_policy, route_reason, rejected_candidates_json, body_bytes,
                        route_wait_ms, upstream_headers_ms, failure_source, attempts_json,
                        completion_source, prompt_tokens, completion_tokens, total_tokens,
                        cache_creation_tokens, cache_read_tokens, reasoning_effort,
                        first_token_ms, finished_at, duration_ms, status
                 FROM request_logs WHERE request_id = ?1",
                ["req-terminal-annotations"],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, Option<String>>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                        row.get::<_, Option<i64>>(15)?,
                        row.get::<_, Option<i64>>(16)?,
                        row.get::<_, Option<i64>>(17)?,
                        row.get::<_, Option<i64>>(18)?,
                        row.get::<_, Option<String>>(19)?,
                        row.get::<_, Option<i64>>(20)?,
                        row.get::<_, Option<String>>(21)?,
                        row.get::<_, Option<i64>>(22)?,
                        row.get::<_, String>(23)?,
                    ))
                },
            )
            .expect("annotations row");

        assert_eq!(observed.0.as_deref(), Some("gpt-test"));
        assert_eq!(observed.1, 1);
        assert_eq!(observed.2.as_deref(), Some("key-1"));
        assert_eq!(observed.3.as_deref(), Some("station-1"));
        assert_eq!(observed.4.as_deref(), Some("https://station.test/v1"));
        assert_eq!(observed.5.as_deref(), Some("stable_first"));
        assert_eq!(observed.6.as_deref(), Some("healthy key"));
        assert_eq!(observed.7.as_deref(), Some("[]"));
        assert_eq!(observed.8, Some(128));
        assert_eq!(observed.9, Some(3));
        assert_eq!(observed.10, Some(7));
        assert_eq!(observed.11.as_deref(), Some("upstream"));
        assert_eq!(observed.12.as_deref(), Some("[]"));
        assert_eq!(observed.13.as_deref(), Some("chat.completion"));
        assert_eq!(observed.14, Some(11));
        assert_eq!(observed.15, Some(13));
        assert_eq!(observed.16, Some(24));
        assert_eq!(observed.17, Some(2));
        assert_eq!(observed.18, Some(5));
        assert_eq!(observed.19.as_deref(), Some("high"));
        assert_eq!(observed.20, Some(17));
        assert!(observed.21.is_some());
        assert!(observed.22.is_some());
        assert_eq!(observed.23, "success");
    }

    fn request_start_record(request_id: &str) -> RequestStartRecord {
        RequestStartRecord {
            context: RequestContextSnapshot {
                request_id: request_id.to_string(),
                method: "POST".to_string(),
                local_path: "/v1/responses".to_string(),
                endpoint: "responses".to_string(),
                received_at_ms: 1000,
            },
        }
    }

    fn file_database(name: &str) -> Result<AppDatabase, String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "relay-pool-persistence-v2-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let default_data_dir: PathBuf = root.join("default");
        let active_data_dir: PathBuf = root.join("active");
        AppDatabase::initialize_new_at(default_data_dir, active_data_dir)
    }

    struct SeededStationKey {
        station_id: String,
        station_key_id: String,
        endpoint_revision: i64,
    }

    fn seed_station_key(database: &AppDatabase, suffix: &str) -> SeededStationKey {
        let station = database
            .create_station(CreateStationInput {
                name: format!("Station {suffix}"),
                station_type: "openai-compatible".to_string(),
                website_url: "https://example.test".to_string(),
                api_base_url: format!("https://{suffix}.example.test/v1"),
                api_key: "sk-station".to_string(),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: format!("Key {suffix}"),
                    api_key: format!("sk-{suffix}"),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: Some(0),
                    load_factor: None,
                    schedulable: Some(true),
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    note: None,
                },
                &[7; 32],
            )
            .expect("station key");
        SeededStationKey {
            station_id: station.id,
            station_key_id: key.id,
            endpoint_revision: station.endpoint_revision,
        }
    }

    fn attempt_record(
        request_id: &str,
        station_id: &str,
        station_key_id: &str,
        endpoint_revision: i64,
        started_at_ms: i64,
    ) -> crate::services::proxy::lifecycle::attempt::AttemptTerminalRecord {
        crate::services::proxy::lifecycle::attempt::AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new(request_id, 0),
                station_id: station_id.to_string(),
                station_key_id: station_key_id.to_string(),
                endpoint_revision,
                started_at_ms,
            },
            terminal: AttemptTerminal::Succeeded,
            output_committed: true,
            terminal_at_ms: 1100,
        }
    }

    fn request_terminal_record(request_id: &str) -> FinalRequestRecord {
        let mut lifecycle = RequestLifecycle::new(RequestContextSnapshot {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/responses".to_string(),
            endpoint: "responses".to_string(),
            received_at_ms: 1000,
        });
        lifecycle.admit().expect("admit");
        let terminal = RequestTerminal::Completed(RequestCompletion {
            protocol_completed: true,
            attempt_id: Some(AttemptId::new(request_id, 0)),
        });
        lifecycle
            .terminalize(terminal, DeliveryTerminal::BodyCompleted)
            .expect("terminal")
    }
}

#[cfg(test)]
mod v2_tests {
    use std::collections::BTreeSet;

    use semver::Version;
    use sqlx::{sqlite::SqlitePoolOptions, Row};

    use crate::{
        persistence::{
            migrations::migrator,
            runtime::PersistenceRuntime,
            schema_compatibility::BinaryCompatibility,
        },
        services::proxy::lifecycle::{
            attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
            delivery::DeliveryTerminal,
            request::{
                AttemptId, FinalRequestRecord, RequestCompletion, RequestContextSnapshot,
                RequestLifecycle, RequestStartRecord, RequestTerminal,
            },
        },
    };

    use super::RequestLogStore;

    fn binary() -> BinaryCompatibility {
        BinaryCompatibility {
            app_version: Version::new(0, 3, 1),
            database_generation: 2,
            readable_schema: 1..=5,
            writable_schema: BTreeSet::from([5]),
        }
    }

    async fn runtime() -> PersistenceRuntime {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool.sqlite3");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}", path.display()))
            .await
            .expect("pool");
        migrator().run(&pool).await.expect("migrations");
        pool.close().await;
        // Keep the directory alive for the lifetime of the test runtime.
        std::mem::forget(root);
        PersistenceRuntime::open(&path, binary()).await.expect("runtime")
    }

    fn start_record(id: &str) -> RequestStartRecord {
        RequestStartRecord {
            context: RequestContextSnapshot {
                request_id: id.to_string(),
                method: "POST".to_string(),
                local_path: "/v1/chat/completions".to_string(),
                endpoint: "chat_completions".to_string(),
                received_at_ms: 1000,
            },
        }
    }

    fn attempt_record(id: &str) -> AttemptTerminalRecord {
        AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new(id, 0),
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 1,
                started_at_ms: 1001,
            },
            terminal: AttemptTerminal::Succeeded,
            output_committed: true,
            terminal_at_ms: 1100,
        }
    }

    fn terminal_record(id: &str) -> FinalRequestRecord {
        let mut lifecycle = RequestLifecycle::new(RequestContextSnapshot {
            request_id: id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/chat/completions".to_string(),
            endpoint: "chat_completions".to_string(),
            received_at_ms: 1000,
        });
        lifecycle.admit().expect("admit");
        lifecycle
            .terminalize(
                RequestTerminal::Completed(RequestCompletion {
                    protocol_completed: true,
                    attempt_id: Some(AttemptId::new(id, 0)),
                }),
                DeliveryTerminal::BodyCompleted,
            )
            .expect("terminal")
    }

    #[tokio::test]
    async fn request_start_is_idempotent() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        let record = start_record("req-start");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(store.start_request(&mut first, &record).await.expect("start").inserted);
        first.commit().await.expect("commit");
        let mut second = runtime.begin_write().await.expect("write");
        assert!(!store.start_request(&mut second, &record).await.expect("duplicate").inserted);
        second.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn attempt_and_health_are_applied_once() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        let record = start_record("req-attempt");
        let mut session = runtime.begin_write().await.expect("write");
        store.start_request(&mut session, &record).await.expect("start");
        session.commit().await.expect("commit");
        let attempt = attempt_record("req-attempt");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(store.finish_attempt(&mut first, &attempt).await.expect("attempt").inserted);
        first.commit().await.expect("commit");
        let mut duplicate = runtime.begin_write().await.expect("write");
        assert!(!store.finish_attempt(&mut duplicate, &attempt).await.expect("duplicate").inserted);
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

    #[tokio::test]
    async fn request_terminal_uses_compare_and_set() {
        let runtime = runtime().await;
        let store = RequestLogStore;
        let record = start_record("req-terminal");
        let mut start = runtime.begin_write().await.expect("write");
        store.start_request(&mut start, &record).await.expect("start");
        start.commit().await.expect("commit");
        let final_record = terminal_record("req-terminal");
        let mut first = runtime.begin_write().await.expect("write");
        assert!(store.finish_request(&mut first, &final_record).await.expect("finish").finalized);
        first.commit().await.expect("commit");
        let mut duplicate = runtime.begin_write().await.expect("write");
        assert!(!store.finish_request(&mut duplicate, &final_record).await.expect("duplicate").finalized);
        duplicate.commit().await.expect("commit");
    }
}
