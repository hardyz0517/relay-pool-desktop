#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-error-rate-history-reference; owner=persistence/stores; remove_when=v3 station-key quality and circuit migration no longer needs legacy history inspection"
    )
)]

use sqlx::{Row, SqliteConnection};

use crate::{
    application::{
        error_rate_protection::{
            ErrorRateHistoryEventV1, ErrorRateHistoryOutcome, ErrorRateHistoryPageV1,
            ErrorRateProtectionConfigV1, HealthProtectionTransitionCode,
        },
        health_protection::{HealthProtectionFailureCode, HealthProtectionScopeKind},
    },
    persistence::error::PersistenceError,
};

const MAX_HISTORY_LIMIT: usize = 4_096;
const MAX_SCOPE_BYTES: usize = 192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorRateHistoryAppendResult {
    Inserted,
    Existing,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingErrorRateHistoryStore;

impl RoutingErrorRateHistoryStore {
    pub(crate) async fn append(
        &self,
        connection: &mut SqliteConnection,
        mut event: ErrorRateHistoryEventV1,
        observation_id: &str,
        config: &ErrorRateProtectionConfigV1,
        now_ms: i64,
    ) -> Result<ErrorRateHistoryAppendResult, PersistenceError> {
        if !config.enabled {
            return Ok(ErrorRateHistoryAppendResult::Existing);
        }
        validate_event(&event, observation_id, now_ms)?;
        if let Some(row) = sqlx::query(
            "SELECT observed_at_ms, scope_kind, scope_commitment, outcome, failure_code, sample_count, failure_count, failure_rate_percent, transition FROM routing_error_rate_history WHERE observation_id = ?1",
        )
        .bind(observation_id)
        .fetch_optional(&mut *connection)
        .await?
        {
            let existing = row_to_event(&row)?;
            if existing == event {
                return Ok(ErrorRateHistoryAppendResult::Existing);
            }
            return Err(PersistenceError::InvariantViolation(
                "error-rate history observation identity collision".into(),
            ));
        }

        prune(connection, config, event.observed_at_ms, now_ms).await?;
        let (sample_count, failure_count) =
            scope_counts(connection, &event.scope_kind, &event.scope_commitment).await?;
        event.sample_count = sample_count.saturating_add(1);
        event.failure_count = failure_count
            .saturating_add(matches!(event.outcome, ErrorRateHistoryOutcome::Failure) as usize);
        event.failure_rate_percent = percentage(event.failure_count, event.sample_count);
        validate_event(&event, observation_id, now_ms)?;

        sqlx::query(
            "INSERT INTO routing_error_rate_history (observation_id, observed_at_ms, scope_kind, scope_commitment, outcome, failure_code, sample_count, failure_count, failure_rate_percent, transition, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        )
        .bind(observation_id)
        .bind(event.observed_at_ms)
        .bind(scope_kind_name(event.scope_kind))
        .bind(&event.scope_commitment)
        .bind(outcome_name(event.outcome.clone()))
        .bind(event.failure_code.map(HealthProtectionFailureCode::as_str))
        .bind(i64::try_from(event.sample_count).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(i64::try_from(event.failure_count).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(i64::from(event.failure_rate_percent))
        .bind(event.transition.map(transition_name))
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;

        prune(connection, config, event.observed_at_ms, now_ms).await?;
        refresh_all_scope_aggregates(connection).await?;
        Ok(ErrorRateHistoryAppendResult::Inserted)
    }

    pub(crate) async fn list_page(
        &self,
        connection: &mut SqliteConnection,
        before_ms: Option<i64>,
        limit: usize,
        config: &ErrorRateProtectionConfigV1,
        now_ms: i64,
    ) -> Result<ErrorRateHistoryPageV1, PersistenceError> {
        if now_ms < 0 || before_ms.is_some_and(|value| value < 0) {
            return Err(PersistenceError::ConstraintViolation);
        }
        let limit = limit.clamp(1, config.history_max_events.min(MAX_HISTORY_LIMIT));
        if !config.enabled {
            return Ok(ErrorRateHistoryPageV1 {
                version: crate::application::error_rate_protection::ERROR_RATE_HISTORY_VERSION
                    .to_string(),
                enabled: false,
                detail_available: false,
                events: Vec::new(),
                next_before_ms: None,
                dropped_events: dropped_events(connection).await?,
            });
        }
        let rows = sqlx::query(
            "SELECT observed_at_ms, scope_kind, scope_commitment, outcome, failure_code, sample_count, failure_count, failure_rate_percent, transition FROM routing_error_rate_history WHERE observed_at_ms >= ?1 AND (?2 IS NULL OR observed_at_ms < ?2) ORDER BY observed_at_ms DESC, ingestion_sequence DESC LIMIT ?3",
        )
        .bind(now_ms.saturating_sub(config.history_retention_ms))
        .bind(before_ms)
        .bind(i64::try_from(limit).map_err(|_| PersistenceError::ConstraintViolation)?)
        .fetch_all(&mut *connection)
        .await?;
        let mut events = rows
            .iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>, _>>()?;
        events.reverse();
        Ok(ErrorRateHistoryPageV1 {
            version: crate::application::error_rate_protection::ERROR_RATE_HISTORY_VERSION
                .to_string(),
            enabled: true,
            detail_available: !events.is_empty(),
            next_before_ms: events.first().map(|event| event.observed_at_ms),
            events,
            dropped_events: dropped_events(connection).await?,
        })
    }
}

async fn scope_counts(
    connection: &mut SqliteConnection,
    kind: &HealthProtectionScopeKind,
    commitment: &str,
) -> Result<(usize, usize), PersistenceError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS sample_count, COALESCE(SUM(CASE WHEN outcome = 'failure' THEN 1 ELSE 0 END), 0) AS failure_count FROM routing_error_rate_history WHERE scope_kind = ?1 AND scope_commitment = ?2",
    )
    .bind(scope_kind_name(*kind))
    .bind(commitment)
    .fetch_one(&mut *connection)
    .await?;
    Ok((
        usize::try_from(row.get::<i64, _>("sample_count")).map_err(|_| {
            PersistenceError::InvariantViolation("negative history sample count".into())
        })?,
        usize::try_from(row.get::<i64, _>("failure_count")).map_err(|_| {
            PersistenceError::InvariantViolation("negative history failure count".into())
        })?,
    ))
}

/// Recompute denormalized counters after retention/size pruning. A row's
/// counters describe the currently retained scope window, so pruning one
/// scope must also refresh rows belonging to every other affected scope.
async fn refresh_all_scope_aggregates(
    connection: &mut SqliteConnection,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "UPDATE routing_error_rate_history AS event SET sample_count = (SELECT COUNT(*) FROM routing_error_rate_history AS scoped WHERE scoped.scope_kind = event.scope_kind AND scoped.scope_commitment = event.scope_commitment), failure_count = (SELECT COUNT(*) FROM routing_error_rate_history AS scoped WHERE scoped.scope_kind = event.scope_kind AND scoped.scope_commitment = event.scope_commitment AND scoped.outcome = 'failure'), failure_rate_percent = (SELECT CASE WHEN COUNT(*) = 0 THEN 0 ELSE MIN(100, (SUM(CASE WHEN scoped.outcome = 'failure' THEN 1 ELSE 0 END) * 100) / COUNT(*)) END FROM routing_error_rate_history AS scoped WHERE scoped.scope_kind = event.scope_kind AND scoped.scope_commitment = event.scope_commitment)",
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn prune(
    connection: &mut SqliteConnection,
    config: &ErrorRateProtectionConfigV1,
    reference_ms: i64,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let cutoff = reference_ms.saturating_sub(config.history_retention_ms);
    let deleted_retention =
        sqlx::query("DELETE FROM routing_error_rate_history WHERE observed_at_ms < ?1")
            .bind(cutoff)
            .execute(&mut *connection)
            .await?
            .rows_affected();
    let max_events = i64::try_from(config.history_max_events)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let deleted_limit = sqlx::query(
        "DELETE FROM routing_error_rate_history WHERE ingestion_sequence IN (SELECT ingestion_sequence FROM routing_error_rate_history ORDER BY ingestion_sequence DESC LIMIT -1 OFFSET ?1)",
    )
    .bind(max_events)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    let dropped = deleted_retention.saturating_add(deleted_limit);
    if dropped > 0 {
        sqlx::query(
            "UPDATE routing_error_rate_history_meta SET dropped_events = dropped_events + ?1, updated_at_ms = ?2 WHERE singleton_key = 1",
        )
        .bind(i64::try_from(dropped).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(now_ms.max(0))
        .execute(&mut *connection)
            .await?;
    }
    if deleted_retention > 0 || deleted_limit > 0 {
        refresh_all_scope_aggregates(connection).await?;
    }
    Ok(())
}

async fn dropped_events(connection: &mut SqliteConnection) -> Result<u64, PersistenceError> {
    let value: i64 = sqlx::query_scalar(
        "SELECT dropped_events FROM routing_error_rate_history_meta WHERE singleton_key = 1",
    )
    .fetch_one(&mut *connection)
    .await?;
    u64::try_from(value)
        .map_err(|_| PersistenceError::InvariantViolation("negative dropped history count".into()))
}

fn validate_event(
    event: &ErrorRateHistoryEventV1,
    observation_id: &str,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    if observation_id.is_empty()
        || observation_id.len() > 160
        || observation_id.chars().any(char::is_control)
        || event.observed_at_ms < 0
        || now_ms < 0
        || event.scope_commitment.len() > MAX_SCOPE_BYTES
        || event.scope_commitment.len() != 64
        || !event
            .scope_commitment
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || event.sample_count == 0
        || event.failure_count > event.sample_count
        || event.failure_rate_percent > 100
        || (matches!(event.outcome, ErrorRateHistoryOutcome::Success)
            && event.failure_code.is_some())
        || (matches!(event.outcome, ErrorRateHistoryOutcome::Failure)
            && event.failure_code.is_none())
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn row_to_event(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ErrorRateHistoryEventV1, PersistenceError> {
    Ok(ErrorRateHistoryEventV1 {
        observed_at_ms: row.get("observed_at_ms"),
        scope_kind: parse_scope_kind(&row.get::<String, _>("scope_kind"))?,
        scope_commitment: row.get("scope_commitment"),
        outcome: parse_outcome(&row.get::<String, _>("outcome"))?,
        failure_code: row
            .get::<Option<String>, _>("failure_code")
            .map(|value| parse_failure_code(&value))
            .transpose()?,
        sample_count: usize::try_from(row.get::<i64, _>("sample_count")).map_err(|_| {
            PersistenceError::InvariantViolation("negative history sample count".into())
        })?,
        failure_count: usize::try_from(row.get::<i64, _>("failure_count")).map_err(|_| {
            PersistenceError::InvariantViolation("negative history failure count".into())
        })?,
        failure_rate_percent: u8::try_from(row.get::<i64, _>("failure_rate_percent")).map_err(
            |_| PersistenceError::InvariantViolation("invalid history failure rate".into()),
        )?,
        transition: row
            .get::<Option<String>, _>("transition")
            .map(|value| parse_transition(&value))
            .transpose()?,
    })
}

fn scope_kind_name(value: HealthProtectionScopeKind) -> &'static str {
    match value {
        HealthProtectionScopeKind::Credential => "credential",
        HealthProtectionScopeKind::Account => "account",
        HealthProtectionScopeKind::Group => "group",
        HealthProtectionScopeKind::Endpoint => "endpoint",
        HealthProtectionScopeKind::Model => "model",
        HealthProtectionScopeKind::CapacityDomain => "capacity_domain",
    }
}

fn parse_scope_kind(value: &str) -> Result<HealthProtectionScopeKind, PersistenceError> {
    match value {
        "credential" => Ok(HealthProtectionScopeKind::Credential),
        "account" => Ok(HealthProtectionScopeKind::Account),
        "group" => Ok(HealthProtectionScopeKind::Group),
        "endpoint" => Ok(HealthProtectionScopeKind::Endpoint),
        "model" => Ok(HealthProtectionScopeKind::Model),
        "capacity_domain" => Ok(HealthProtectionScopeKind::CapacityDomain),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown error-rate scope kind".into(),
        )),
    }
}

fn outcome_name(value: ErrorRateHistoryOutcome) -> &'static str {
    match value {
        ErrorRateHistoryOutcome::Success => "success",
        ErrorRateHistoryOutcome::Failure => "failure",
    }
}

fn parse_outcome(value: &str) -> Result<ErrorRateHistoryOutcome, PersistenceError> {
    match value {
        "success" => Ok(ErrorRateHistoryOutcome::Success),
        "failure" => Ok(ErrorRateHistoryOutcome::Failure),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown error-rate outcome".into(),
        )),
    }
}

fn parse_failure_code(value: &str) -> Result<HealthProtectionFailureCode, PersistenceError> {
    let code = crate::application::health_protection::failure_code_from_label(value);
    if code.as_str() == value {
        Ok(code)
    } else {
        Err(PersistenceError::InvariantViolation(
            "unknown error-rate failure code".into(),
        ))
    }
}

fn transition_name(value: HealthProtectionTransitionCode) -> &'static str {
    match value {
        HealthProtectionTransitionCode::IgnoredDuplicate => "ignored_duplicate",
        HealthProtectionTransitionCode::Observed => "observed",
        HealthProtectionTransitionCode::Opened => "opened",
        HealthProtectionTransitionCode::ProbeSucceeded => "probe_succeeded",
        HealthProtectionTransitionCode::Closed => "closed",
        HealthProtectionTransitionCode::Reopened => "reopened",
    }
}

fn parse_transition(value: &str) -> Result<HealthProtectionTransitionCode, PersistenceError> {
    match value {
        "ignored_duplicate" => Ok(HealthProtectionTransitionCode::IgnoredDuplicate),
        "observed" => Ok(HealthProtectionTransitionCode::Observed),
        "opened" => Ok(HealthProtectionTransitionCode::Opened),
        "probe_succeeded" => Ok(HealthProtectionTransitionCode::ProbeSucceeded),
        "closed" => Ok(HealthProtectionTransitionCode::Closed),
        "reopened" => Ok(HealthProtectionTransitionCode::Reopened),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown error-rate transition".into(),
        )),
    }
}

fn percentage(failures: usize, samples: usize) -> u8 {
    if samples == 0 {
        0
    } else {
        ((failures.saturating_mul(100) / samples).min(100)) as u8
    }
}
