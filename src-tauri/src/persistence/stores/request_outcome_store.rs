use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[cfg(not(test))]
use super::dashboard_metrics_rollup::record_cost_aggregate_rollup;
pub(crate) use super::request_cost_write::RequestCostAggregateWrite;

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

impl RequestOutcomeStore {
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
