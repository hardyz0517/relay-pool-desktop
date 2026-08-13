use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[cfg(not(test))]
use super::dashboard_metrics_rollup::record_cost_aggregate_rollup;
pub(crate) use super::request_cost_write::RequestCostAggregateWrite;
use super::request_log_write::RequestRoutingOutcomeSummaryWrite;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RequestOutcomeStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCostWrite {
    pub(crate) request_id: String,
    pub(crate) ordinal: u16,
    pub(crate) pricing_context_id: String,
    pub(crate) pricing_basis: String,
    pub(crate) pricing_status_label: String,
    pub(crate) usage_status: String,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) cache_creation_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
    pub(crate) cost_status: String,
    pub(crate) currency: Option<String>,
    pub(crate) total_cost_micro: Option<i64>,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InsertAck {
    pub(crate) inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingOutcomeSummaryRow {
    pub(crate) request_id: String,
    pub(crate) profile_version: String,
    pub(crate) terminal_kind: String,
    pub(crate) terminal_code: String,
    pub(crate) classification: String,
    pub(crate) confidence: String,
    pub(crate) evidence_source: String,
    pub(crate) request_accepted: String,
    pub(crate) send_phase: String,
    pub(crate) replay_disposition: String,
    pub(crate) billing_state: String,
    pub(crate) retry_disposition: String,
    pub(crate) effect_summary: String,
    pub(crate) failure_domain_commitment_version: Option<i64>,
    pub(crate) failure_domain_commitment_digest: Option<String>,
    pub(crate) attempt_count: i64,
    pub(crate) fallback_count: i64,
    pub(crate) terminal_at_ms: i64,
}

impl RequestOutcomeStore {
    pub(crate) async fn routing_outcome_summary(
        &self,
        connection: &mut SqliteConnection,
        request_id: &str,
    ) -> Result<Option<RoutingOutcomeSummaryRow>, PersistenceError> {
        let row = sqlx::query(
            "SELECT request_id, profile_version, terminal_kind, terminal_code, classification,
                    confidence, evidence_source, request_accepted, send_phase, replay_disposition,
                    billing_state, retry_disposition, effect_summary, failure_domain_commitment_version,
                    failure_domain_commitment_digest, attempt_count, fallback_count, terminal_at_ms
             FROM request_routing_outcome_summaries WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_optional(&mut *connection)
        .await?;
        Ok(row.map(|row| RoutingOutcomeSummaryRow {
            request_id: row.get(0),
            profile_version: row.get(1),
            terminal_kind: row.get(2),
            terminal_code: row.get(3),
            classification: row.get(4),
            confidence: row.get(5),
            evidence_source: row.get(6),
            request_accepted: row.get(7),
            send_phase: row.get(8),
            replay_disposition: row.get(9),
            billing_state: row.get(10),
            retry_disposition: row.get(11),
            effect_summary: row.get(12),
            failure_domain_commitment_version: row.get(13),
            failure_domain_commitment_digest: row.get(14),
            attempt_count: row.get(15),
            fallback_count: row.get(16),
            terminal_at_ms: row.get(17),
        }))
    }

    pub(crate) async fn insert_routing_outcome_summary(
        &self,
        connection: &mut SqliteConnection,
        request_id: &str,
        record: &RequestRoutingOutcomeSummaryWrite,
    ) -> Result<InsertAck, PersistenceError> {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO request_routing_outcome_summaries (
                request_id, profile_version, terminal_kind, terminal_code, classification,
                confidence, evidence_source, request_accepted, send_phase, replay_disposition,
                billing_state, retry_disposition, effect_summary, failure_domain_commitment_version,
                failure_domain_commitment_digest, attempt_count, fallback_count, terminal_at_ms
             ) VALUES (?, 'routing_outcome_v1', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(request_id)
        .bind(&record.terminal_kind)
        .bind(&record.terminal_code)
        .bind(&record.classification)
        .bind(&record.confidence)
        .bind(&record.evidence_source)
        .bind(&record.request_accepted)
        .bind(&record.send_phase)
        .bind(&record.replay_disposition)
        .bind(&record.billing_state)
        .bind(&record.retry_disposition)
        .bind(&record.effect_summary)
        .bind(record.failure_domain_commitment_version)
        .bind(record.failure_domain_commitment_digest.as_deref())
        .bind(i64::from(record.attempt_count))
        .bind(i64::from(record.fallback_count))
        .bind(record.terminal_at_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if inserted > 0 {
            return Ok(InsertAck { inserted: true });
        }
        let existing = sqlx::query(
            "SELECT terminal_kind, terminal_code, classification, confidence, evidence_source,
                    request_accepted, send_phase, replay_disposition, billing_state, retry_disposition,
                    effect_summary, failure_domain_commitment_version, failure_domain_commitment_digest,
                    attempt_count, fallback_count, terminal_at_ms
             FROM request_routing_outcome_summaries WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "duplicate routing outcome missing canonical row".to_string(),
            )
        })?;
        let matches = existing.get::<String, _>(0) == record.terminal_kind
            && existing.get::<String, _>(1) == record.terminal_code
            && existing.get::<String, _>(2) == record.classification
            && existing.get::<String, _>(3) == record.confidence
            && existing.get::<String, _>(4) == record.evidence_source
            && existing.get::<String, _>(5) == record.request_accepted
            && existing.get::<String, _>(6) == record.send_phase
            && existing.get::<String, _>(7) == record.replay_disposition
            && existing.get::<String, _>(8) == record.billing_state
            && existing.get::<String, _>(9) == record.retry_disposition
            && existing.get::<String, _>(10) == record.effect_summary
            && existing.get::<Option<i64>, _>(11) == record.failure_domain_commitment_version
            && existing.get::<Option<String>, _>(12) == record.failure_domain_commitment_digest
            && existing.get::<i64, _>(13) == i64::from(record.attempt_count)
            && existing.get::<i64, _>(14) == i64::from(record.fallback_count);
        // The first projection fixes `terminal_at_ms`; it is audit metadata,
        // not part of the terminal's idempotency identity.
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "duplicate routing outcome does not match canonical record".to_string(),
            ));
        }
        Ok(InsertAck { inserted: false })
    }

    pub(crate) async fn insert_attempt_cost(
        &self,
        connection: &mut SqliteConnection,
        record: &AttemptCostWrite,
    ) -> Result<InsertAck, PersistenceError> {
        validate_attempt_cost(record)?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO routing_attempt_costs (
                request_id, ordinal, pricing_context_id, pricing_basis, pricing_status_label,
                usage_status, input_tokens, output_tokens, total_tokens, cache_creation_tokens,
                cache_read_tokens, cost_status, currency, total_cost_micro, created_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.request_id)
        .bind(i64::from(record.ordinal))
        .bind(&record.pricing_context_id)
        .bind(&record.pricing_basis)
        .bind(&record.pricing_status_label)
        .bind(&record.usage_status)
        .bind(record.input_tokens)
        .bind(record.output_tokens)
        .bind(record.total_tokens)
        .bind(record.cache_creation_tokens)
        .bind(record.cache_read_tokens)
        .bind(&record.cost_status)
        .bind(record.currency.as_deref())
        .bind(record.total_cost_micro)
        .bind(record.created_at_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();

        if inserted > 0 {
            return Ok(InsertAck { inserted: true });
        }

        let existing =
            attempt_cost_by_identity(connection, &record.request_id, record.ordinal).await?;
        let existing = existing.ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "duplicate attempt cost missing canonical row".to_string(),
            )
        })?;
        if existing != *record {
            return Err(PersistenceError::InvariantViolation(
                "duplicate attempt cost does not match canonical record".to_string(),
            ));
        }
        Ok(InsertAck { inserted: false })
    }

    pub(crate) async fn insert_request_cost_aggregate(
        &self,
        connection: &mut SqliteConnection,
        record: &RequestCostAggregateWrite,
    ) -> Result<InsertAck, PersistenceError> {
        validate_request_aggregate(record)?;
        ensure_all_started_attempt_costs_exist(connection, &record.request_id).await?;

        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO routing_request_cost_aggregates (
                request_id, status, totals_by_currency_json, compatibility_currency,
                compatibility_total_cost_micro, incomplete_attempts_json, created_at_ms, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.request_id)
        .bind(&record.status)
        .bind(&record.totals_by_currency_json)
        .bind(record.compatibility_currency.as_deref())
        .bind(record.compatibility_total_cost_micro)
        .bind(&record.incomplete_attempts_json)
        .bind(record.written_at_ms)
        .bind(record.written_at_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();

        if inserted == 0 {
            let existing = request_cost_aggregate_by_id(connection, &record.request_id).await?;
            let existing = existing.ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "duplicate request cost aggregate missing canonical row".to_string(),
                )
            })?;
            if existing != *record {
                return Err(PersistenceError::InvariantViolation(
                    "duplicate request cost aggregate does not match canonical record".to_string(),
                ));
            }
            return Ok(InsertAck { inserted: false });
        }

        update_request_log_cost_projection(connection, record).await?;
        #[cfg(not(test))]
        record_cost_aggregate_rollup(connection, record).await?;
        Ok(InsertAck { inserted: true })
    }
}

fn validate_attempt_cost(record: &AttemptCostWrite) -> Result<(), PersistenceError> {
    let priced = record.cost_status == "priced";
    if priced
        != (record.currency.is_some()
            && record.total_cost_micro.is_some()
            && record.usage_status == "complete")
    {
        return Err(PersistenceError::InvariantViolation(
            "attempt cost priced state must include currency, total and complete usage".to_string(),
        ));
    }
    Ok(())
}

fn validate_request_aggregate(record: &RequestCostAggregateWrite) -> Result<(), PersistenceError> {
    let single = record.status == "complete_single_currency";
    if single
        != (record.compatibility_currency.is_some()
            && record.compatibility_total_cost_micro.is_some())
    {
        return Err(PersistenceError::InvariantViolation(
            "single-currency aggregate must be the only compatibility projection".to_string(),
        ));
    }
    Ok(())
}

async fn ensure_all_started_attempt_costs_exist(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<(), PersistenceError> {
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM request_attempts WHERE request_id = ?")
            .bind(request_id)
            .fetch_one(&mut *connection)
            .await?;
    let cost_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routing_attempt_costs WHERE request_id = ?")
            .bind(request_id)
            .fetch_one(&mut *connection)
            .await?;
    if cost_count != attempt_count {
        return Err(PersistenceError::InvariantViolation(format!(
            "request aggregate requires all durable attempt costs: attempts={attempt_count}, costs={cost_count}"
        )));
    }
    Ok(())
}

async fn update_request_log_cost_projection(
    connection: &mut SqliteConnection,
    record: &RequestCostAggregateWrite,
) -> Result<(), PersistenceError> {
    let estimated_total_cost = record
        .compatibility_total_cost_micro
        .map(|value| value as f64 / 1_000_000.0);
    sqlx::query(
        "UPDATE request_logs
         SET cost_status = ?, cost_currency = ?, estimated_total_cost = ?
         WHERE request_id = ?",
    )
    .bind(&record.status)
    .bind(record.compatibility_currency.as_deref())
    .bind(estimated_total_cost)
    .bind(&record.request_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn attempt_cost_by_identity(
    connection: &mut SqliteConnection,
    request_id: &str,
    ordinal: u16,
) -> Result<Option<AttemptCostWrite>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_id, ordinal, pricing_context_id, pricing_basis, pricing_status_label,
                usage_status, input_tokens, output_tokens, total_tokens, cache_creation_tokens,
                cache_read_tokens, cost_status, currency, total_cost_micro, created_at_ms
         FROM routing_attempt_costs
         WHERE request_id = ? AND ordinal = ?",
    )
    .bind(request_id)
    .bind(i64::from(ordinal))
    .fetch_optional(&mut *connection)
    .await?;

    Ok(row.map(|row| AttemptCostWrite {
        request_id: row.get(0),
        ordinal: row.get::<i64, _>(1) as u16,
        pricing_context_id: row.get(2),
        pricing_basis: row.get(3),
        pricing_status_label: row.get(4),
        usage_status: row.get(5),
        input_tokens: row.get(6),
        output_tokens: row.get(7),
        total_tokens: row.get(8),
        cache_creation_tokens: row.get(9),
        cache_read_tokens: row.get(10),
        cost_status: row.get(11),
        currency: row.get(12),
        total_cost_micro: row.get(13),
        created_at_ms: row.get(14),
    }))
}

async fn request_cost_aggregate_by_id(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<Option<RequestCostAggregateWrite>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_id, status, totals_by_currency_json, compatibility_currency,
                compatibility_total_cost_micro, incomplete_attempts_json, created_at_ms
         FROM routing_request_cost_aggregates
         WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&mut *connection)
    .await?;

    Ok(row.map(|row| RequestCostAggregateWrite {
        request_id: row.get(0),
        status: row.get(1),
        totals_by_currency_json: row.get(2),
        compatibility_currency: row.get(3),
        compatibility_total_cost_micro: row.get(4),
        incomplete_attempts_json: row.get(5),
        written_at_ms: row.get(6),
    }))
}
