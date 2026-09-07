use sqlx::{Row, SqliteConnection};

use crate::persistence::{
    error::PersistenceError,
    stores::routing_attempt_store::{FinalizedRoutingAttemptSample, RoutingAttemptStore},
};

const DEFAULT_STARTUP_RECONCILIATION_BATCH_SIZE: u32 = 64;
const MAX_STARTUP_RECONCILIATION_BATCH_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartupReconciliationReport {
    pub(crate) batches_completed: u64,
    pub(crate) requests_interrupted: u64,
    pub(crate) attempt_cost_gaps_inserted: u64,
    pub(crate) decisions_marked_trace_incomplete: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupReconciliationBatch {
    pub(crate) report: StartupReconciliationReport,
    pub(crate) has_more: bool,
    pub(crate) routing_samples: Vec<FinalizedRoutingAttemptSample>,
}

impl StartupReconciliationReport {
    pub(crate) fn empty() -> Self {
        Self {
            batches_completed: 0,
            requests_interrupted: 0,
            attempt_cost_gaps_inserted: 0,
            decisions_marked_trace_incomplete: 0,
        }
    }

    pub(crate) fn add_batch(&mut self, batch: StartupReconciliationBatch) {
        self.batches_completed += batch.report.batches_completed;
        self.requests_interrupted += batch.report.requests_interrupted;
        self.attempt_cost_gaps_inserted += batch.report.attempt_cost_gaps_inserted;
        self.decisions_marked_trace_incomplete += batch.report.decisions_marked_trace_incomplete;
    }
}

pub(crate) fn default_startup_reconciliation_batch_size() -> u32 {
    DEFAULT_STARTUP_RECONCILIATION_BATCH_SIZE
}

pub(crate) async fn reconcile_startup_interrupted_batch(
    connection: &mut SqliteConnection,
    now_ms: i64,
    batch_size: u32,
) -> Result<StartupReconciliationBatch, PersistenceError> {
    let batch_size = batch_size
        .clamp(1, MAX_STARTUP_RECONCILIATION_BATCH_SIZE)
        .saturating_add(1);
    let rows = sqlx::query(
        r#"
        SELECT request_id
        FROM request_logs
        WHERE terminal_at_ms IS NULL
          AND status = 'in_progress'
          AND COALESCE(lifecycle_status, 'admitted') IN ('admitted', 'routing', 'attempting', 'in_progress')
        ORDER BY request_id ASC
        LIMIT ?1
        "#,
    )
    .bind(i64::from(batch_size))
    .fetch_all(&mut *connection)
    .await?;

    let requested = usize::try_from(batch_size.saturating_sub(1)).unwrap_or(usize::MAX);
    let mut has_more = rows.len() > requested;
    let request_ids = rows
        .into_iter()
        .take(requested)
        .map(|row| row.get::<String, _>(0))
        .collect::<Vec<_>>();

    if request_ids.is_empty() {
        return backfill_missing_cost_aggregates_batch(connection, now_ms, requested).await;
    }

    let mut report = StartupReconciliationReport {
        batches_completed: 1,
        ..StartupReconciliationReport::empty()
    };
    let mut last_request_id = None::<String>;
    let mut routing_samples = Vec::new();
    let mut processed = 0usize;
    for request_id in request_ids {
        report.attempt_cost_gaps_inserted +=
            insert_trace_incomplete_attempt_costs(connection, &request_id, now_ms).await?;
        report.decisions_marked_trace_incomplete +=
            mark_route_decision_trace_incomplete(connection, &request_id, now_ms).await?;
        let routing_attempt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_attempt_v3
             WHERE source = 'real_request' AND correlation_id = ?1
               AND candidate_admitted = 1",
        )
        .bind(&request_id)
        .fetch_one(&mut *connection)
        .await?;
        if routing_attempt_count > 0 {
            RoutingAttemptStore::recover_startup_interrupted(
                connection,
                &request_id,
                now_ms.max(0),
            )
            .await?;
            routing_samples.extend(
                RoutingAttemptStore::finalize_request_clusters(
                    connection,
                    &request_id,
                    now_ms.max(0),
                )
                .await?,
            );
        }
        let interrupted = interrupt_request_log(connection, &request_id, now_ms).await?;
        report.requests_interrupted += interrupted;
        if interrupted > 0 {
            // Startup reconciliation is itself the terminal owner for a
            // request whose process died before the normal outbox projection.
            // Materialize the same redacted summary/event contract so the
            // request detail query remains useful after restart.
            materialize_interrupted_outcome(connection, &request_id).await?;
        }
        backfill_request_cost_aggregate(connection, &request_id, now_ms).await?;
        last_request_id = Some(request_id);
        processed += 1;
    }

    if !has_more {
        let remaining = requested.saturating_sub(processed);
        if remaining > 0 {
            let extra =
                backfill_missing_cost_aggregates(connection, now_ms, remaining.saturating_add(1))
                    .await?;
            has_more = extra.has_more;
            last_request_id = extra.last_request_id.or(last_request_id);
        } else {
            has_more = terminal_missing_cost_aggregates_exist(connection).await?;
        }
    }

    record_reconciliation_progress(
        connection,
        last_request_id.as_deref(),
        now_ms,
        report,
        !has_more,
    )
    .await?;

    Ok(StartupReconciliationBatch {
        report,
        has_more,
        routing_samples,
    })
}

async fn insert_trace_incomplete_attempt_costs(
    connection: &mut SqliteConnection,
    request_id: &str,
    now_ms: i64,
) -> Result<u64, PersistenceError> {
    let inserted = sqlx::query(
        r#"
        INSERT OR IGNORE INTO routing_attempt_costs (
            request_id, ordinal, pricing_context_id, pricing_basis, pricing_status_label,
            usage_status, input_tokens, output_tokens, total_tokens, cache_creation_tokens,
            cache_read_tokens, cost_status, currency, total_cost_micro, created_at_ms
        )
        SELECT a.request_id, a.ordinal, 'trace_incomplete', 'unpriced', 'trace_incomplete',
               'missing_usage', NULL, NULL, NULL, NULL, NULL, 'missing_usage', NULL, NULL, ?2
        FROM request_attempts a
        LEFT JOIN routing_attempt_costs c
          ON c.request_id = a.request_id AND c.ordinal = a.ordinal
        WHERE a.request_id = ?1
          AND c.request_id IS NULL
        "#,
    )
    .bind(request_id)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(inserted)
}

async fn mark_route_decision_trace_incomplete(
    connection: &mut SqliteConnection,
    request_id: &str,
    now_ms: i64,
) -> Result<u64, PersistenceError> {
    let updated = sqlx::query(
        r#"
        UPDATE route_decisions
        SET trace_status = 'trace_incomplete',
            updated_at_ms = ?2
        WHERE request_id = ?1
          AND trace_status <> 'trace_incomplete'
        "#,
    )
    .bind(request_id)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(updated)
}

async fn interrupt_request_log(
    connection: &mut SqliteConnection,
    request_id: &str,
    now_ms: i64,
) -> Result<u64, PersistenceError> {
    let updated = sqlx::query(
        r#"
        UPDATE request_logs
        SET finished_at = ?2,
            duration_ms = CASE
                WHEN CAST(started_at AS INTEGER) > 0 THEN MAX(?2 - CAST(started_at AS INTEGER), 0)
                ELSE duration_ms
            END,
            status = 'interrupted',
            lifecycle_status = 'interrupted',
            usage_status = 'missing_usage',
            error_message = COALESCE(error_message, 'request lifecycle interrupted during previous process'),
            terminal_kind = 'interrupted',
            terminal_code = 'startup_interrupted',
            terminal_detail = COALESCE(terminal_detail, 'request lifecycle was marked trace_incomplete during startup reconciliation'),
            protocol_completed = 0,
            delivery_terminal = 'NotStarted',
            terminal_at_ms = ?2,
            cost_status = COALESCE(cost_status, 'incomplete')
        WHERE request_id = ?1
          AND terminal_at_ms IS NULL
          AND status = 'in_progress'
        "#,
    )
    .bind(request_id)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(updated)
}

async fn materialize_interrupted_outcome(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO request_routing_outcome_summaries (
            request_id, profile_version, terminal_kind, terminal_code,
            classification, confidence, evidence_source, request_accepted,
            send_phase, replay_disposition, billing_state, retry_disposition,
            effect_summary, failure_domain_commitment_version,
            failure_domain_commitment_digest, attempt_count, fallback_count,
            terminal_at_ms
        )
        SELECT l.request_id, 'routing_outcome_v1', 'interrupted',
               'startup_interrupted', 'local', 'confirmed', 'local',
               'unknown', 'unknown', 'stopped_uncertain', 'possibly_billed',
               'fail_closed', 'none', NULL, NULL,
               COALESCE(
                   l.attempt_count,
                   (SELECT COUNT(*) FROM request_attempts a
                    WHERE a.request_id = l.request_id),
                   0
               ),
               COALESCE(l.fallback_count, 0), l.terminal_at_ms
        FROM request_logs l
        WHERE l.request_id = ?
          AND l.terminal_kind = 'interrupted'
        "#,
    )
    .bind(request_id)
    .execute(&mut *connection)
    .await?;

    // Keep the lifecycle event bounded by the same per-request cap as normal
    // finalization.  INSERT OR IGNORE makes startup reconciliation idempotent
    // if a process is interrupted again while the repair is running.
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO request_decision_events (
            request_id, event_key, sequence, occurred_at_ms, event_kind,
            detail_code, attempt_ordinal, retry_disposition, output_committed
        )
        SELECT l.request_id, 'request_finalized',
               COALESCE((
                   SELECT MAX(e.sequence) + 1
                   FROM request_decision_events e
                   WHERE e.request_id = l.request_id
               ), 0),
               l.terminal_at_ms, 'request_finalized', 'startup_interrupted',
               NULL, 'stop_request', 0
        FROM request_logs l
        WHERE l.request_id = ?
          AND l.terminal_kind = 'interrupted'
          AND (SELECT COUNT(*) FROM request_decision_events e
               WHERE e.request_id = l.request_id) < 64
        "#,
    )
    .bind(request_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn mark_reconciliation_completed(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    record_reconciliation_progress(
        connection,
        None,
        now_ms,
        StartupReconciliationReport::empty(),
        true,
    )
    .await
}

async fn record_reconciliation_progress(
    connection: &mut SqliteConnection,
    last_request_id: Option<&str>,
    now_ms: i64,
    report: StartupReconciliationReport,
    completed: bool,
) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        INSERT INTO routing_lifecycle_reconciliation_progress (
            singleton_key, last_request_id, last_run_at_ms, batches_completed,
            requests_interrupted, attempt_cost_gaps_inserted,
            decisions_marked_trace_incomplete, completed
        ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(singleton_key) DO UPDATE SET
            last_request_id = COALESCE(excluded.last_request_id, routing_lifecycle_reconciliation_progress.last_request_id),
            last_run_at_ms = excluded.last_run_at_ms,
            batches_completed = routing_lifecycle_reconciliation_progress.batches_completed + excluded.batches_completed,
            requests_interrupted = routing_lifecycle_reconciliation_progress.requests_interrupted + excluded.requests_interrupted,
            attempt_cost_gaps_inserted = routing_lifecycle_reconciliation_progress.attempt_cost_gaps_inserted + excluded.attempt_cost_gaps_inserted,
            decisions_marked_trace_incomplete = routing_lifecycle_reconciliation_progress.decisions_marked_trace_incomplete + excluded.decisions_marked_trace_incomplete,
            completed = excluded.completed
        "#,
    )
    .bind(last_request_id)
    .bind(now_ms)
    .bind(i64::try_from(report.batches_completed).unwrap_or(i64::MAX))
    .bind(i64::try_from(report.requests_interrupted).unwrap_or(i64::MAX))
    .bind(i64::try_from(report.attempt_cost_gaps_inserted).unwrap_or(i64::MAX))
    .bind(i64::try_from(report.decisions_marked_trace_incomplete).unwrap_or(i64::MAX))
    .bind(i64::from(completed as u8))
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn backfill_missing_cost_aggregates_batch(
    connection: &mut SqliteConnection,
    now_ms: i64,
    requested: usize,
) -> Result<StartupReconciliationBatch, PersistenceError> {
    let extra =
        backfill_missing_cost_aggregates(connection, now_ms, requested.saturating_add(1)).await?;
    if extra.processed == 0 && !extra.has_more {
        mark_reconciliation_completed(connection, now_ms).await?;
        return Ok(StartupReconciliationBatch {
            report: StartupReconciliationReport::empty(),
            has_more: false,
            routing_samples: Vec::new(),
        });
    }

    let report = StartupReconciliationReport {
        batches_completed: 1,
        ..StartupReconciliationReport::empty()
    };
    record_reconciliation_progress(
        connection,
        extra.last_request_id.as_deref(),
        now_ms,
        report,
        !extra.has_more,
    )
    .await?;
    Ok(StartupReconciliationBatch {
        report,
        has_more: extra.has_more,
        routing_samples: Vec::new(),
    })
}

struct MissingAggregateBackfill {
    processed: u64,
    has_more: bool,
    last_request_id: Option<String>,
}

async fn backfill_missing_cost_aggregates(
    connection: &mut SqliteConnection,
    now_ms: i64,
    limit: usize,
) -> Result<MissingAggregateBackfill, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT l.id
        FROM request_logs l
        LEFT JOIN routing_request_cost_aggregates a ON a.request_id = l.id
        WHERE l.terminal_at_ms IS NOT NULL
          AND a.request_id IS NULL
        ORDER BY l.id ASC
        LIMIT ?1
        "#,
    )
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(&mut *connection)
    .await?;

    let requested = limit.saturating_sub(1);
    let has_more = rows.len() > requested;
    let request_ids = rows
        .into_iter()
        .take(requested)
        .map(|row| row.get::<String, _>(0))
        .collect::<Vec<_>>();

    let mut last_request_id = None::<String>;
    let mut processed = 0u64;
    for request_id in request_ids {
        insert_trace_incomplete_attempt_costs(connection, &request_id, now_ms).await?;
        backfill_request_cost_aggregate(connection, &request_id, now_ms).await?;
        processed += 1;
        last_request_id = Some(request_id);
    }
    Ok(MissingAggregateBackfill {
        processed,
        has_more,
        last_request_id,
    })
}

async fn terminal_missing_cost_aggregates_exist(
    connection: &mut SqliteConnection,
) -> Result<bool, PersistenceError> {
    let exists: i64 = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM request_logs l
            LEFT JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            WHERE l.terminal_at_ms IS NOT NULL
              AND a.request_id IS NULL
        )
        "#,
    )
    .fetch_one(&mut *connection)
    .await?;
    Ok(exists != 0)
}

async fn backfill_request_cost_aggregate(
    connection: &mut SqliteConnection,
    request_id: &str,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_request_cost_aggregates WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_one(&mut *connection)
    .await?;
    if existing > 0 {
        project_existing_cost_aggregate(connection, request_id).await?;
        return Ok(());
    }

    let ordinals: Vec<i64> = sqlx::query_scalar(
        "SELECT ordinal FROM request_attempts WHERE request_id = ? ORDER BY ordinal ASC",
    )
    .bind(request_id)
    .fetch_all(&mut *connection)
    .await?;

    let cost_rows = sqlx::query(
        r#"
        SELECT ordinal, cost_status, currency, total_cost_micro
        FROM routing_attempt_costs
        WHERE request_id = ?
        ORDER BY ordinal ASC
        "#,
    )
    .bind(request_id)
    .fetch_all(&mut *connection)
    .await?;

    let mut totals = serde_json::Map::new();
    let mut incomplete = Vec::new();
    let mut priced_count = 0usize;
    let mut not_applicable_count = 0usize;
    for row in &cost_rows {
        let ordinal: i64 = row.get(0);
        let status: String = row.get(1);
        let currency: Option<String> = row.get(2);
        let total_cost_micro: Option<i64> = row.get(3);
        match status.as_str() {
            "priced" => {
                if let (Some(currency), Some(amount)) = (currency, total_cost_micro) {
                    let entry = totals.entry(currency).or_insert(serde_json::json!(0));
                    if let Some(current) = entry.as_i64() {
                        *entry = serde_json::json!(current.saturating_add(amount));
                    }
                    priced_count += 1;
                } else {
                    incomplete.push(serde_json::json!({
                        "request_id": request_id,
                        "ordinal": ordinal,
                        "status": status,
                    }));
                }
            }
            "not_applicable" => not_applicable_count += 1,
            other => incomplete.push(serde_json::json!({
                "request_id": request_id,
                "ordinal": ordinal,
                "status": other,
            })),
        }
    }

    for ordinal in &ordinals {
        let has_cost = cost_rows.iter().any(|row| row.get::<i64, _>(0) == *ordinal);
        if !has_cost {
            incomplete.push(serde_json::json!({
                "request_id": request_id,
                "ordinal": *ordinal,
                "status": "missing_usage",
            }));
        }
    }

    let (status, compatibility_currency, compatibility_total_cost_micro) =
        if ordinals.is_empty() && cost_rows.is_empty() {
            ("no_attempts", None, None)
        } else if !incomplete.is_empty() {
            ("incomplete", None, None)
        } else if priced_count == 0 && not_applicable_count == cost_rows.len() {
            ("not_applicable", None, None)
        } else if totals.len() == 1 {
            let (currency, amount) = totals.iter().next().expect("one currency");
            (
                "complete_single_currency",
                Some(currency.clone()),
                amount.as_i64(),
            )
        } else {
            ("complete_mixed_currency", None, None)
        };

    let totals_by_currency_json = serde_json::Value::Object(totals).to_string();
    let incomplete_attempts_json = serde_json::Value::Array(incomplete).to_string();
    let estimated_total_cost =
        compatibility_total_cost_micro.map(|value| value as f64 / 1_000_000.0);

    let inserted = sqlx::query(
        r#"
        INSERT OR IGNORE INTO routing_request_cost_aggregates (
            request_id, status, totals_by_currency_json, compatibility_currency,
            compatibility_total_cost_micro, incomplete_attempts_json, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(request_id)
    .bind(status)
    .bind(&totals_by_currency_json)
    .bind(compatibility_currency.as_deref())
    .bind(compatibility_total_cost_micro)
    .bind(&incomplete_attempts_json)
    .bind(now_ms)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?
    .rows_affected();

    sqlx::query(
        "UPDATE request_logs
         SET cost_status = ?, cost_currency = ?, estimated_total_cost = ?
         WHERE request_id = ? OR id = ?",
    )
    .bind(status)
    .bind(compatibility_currency.as_deref())
    .bind(estimated_total_cost)
    .bind(request_id)
    .bind(request_id)
    .execute(&mut *connection)
    .await?;

    if inserted > 0 {
        record_backfilled_cost_aggregate_rollup(
            connection,
            request_id,
            status,
            &totals_by_currency_json,
            compatibility_currency.as_deref(),
            compatibility_total_cost_micro,
            &incomplete_attempts_json,
            now_ms,
        )
        .await?;
    }
    Ok(())
}

async fn project_existing_cost_aggregate(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT status, compatibility_currency, compatibility_total_cost_micro
         FROM routing_request_cost_aggregates
         WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let status: String = row.get(0);
    let currency: Option<String> = row.get(1);
    let total_micro: Option<i64> = row.get(2);
    let estimated_total_cost = total_micro.map(|value| value as f64 / 1_000_000.0);
    sqlx::query(
        "UPDATE request_logs
         SET cost_status = COALESCE(cost_status, ?),
             cost_currency = COALESCE(cost_currency, ?),
             estimated_total_cost = COALESCE(estimated_total_cost, ?)
         WHERE request_id = ? OR id = ?",
    )
    .bind(status)
    .bind(currency.as_deref())
    .bind(estimated_total_cost)
    .bind(request_id)
    .bind(request_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn record_backfilled_cost_aggregate_rollup(
    connection: &mut SqliteConnection,
    request_id: &str,
    status: &str,
    totals_by_currency_json: &str,
    compatibility_currency: Option<&str>,
    compatibility_total_cost_micro: Option<i64>,
    incomplete_attempts_json: &str,
    written_at_ms: i64,
) -> Result<(), PersistenceError> {
    #[cfg(test)]
    {
        let _ = (
            connection,
            request_id,
            status,
            totals_by_currency_json,
            compatibility_currency,
            compatibility_total_cost_micro,
            incomplete_attempts_json,
            written_at_ms,
        );
    }
    #[cfg(not(test))]
    {
        use crate::persistence::stores::{
            dashboard_metrics_rollup::record_cost_aggregate_rollup,
            request_cost_write::RequestCostAggregateWrite,
        };
        let record = RequestCostAggregateWrite {
            request_id: request_id.to_string(),
            status: status.to_string(),
            totals_by_currency_json: totals_by_currency_json.to_string(),
            compatibility_currency: compatibility_currency.map(ToString::to_string),
            compatibility_total_cost_micro,
            incomplete_attempts_json: incomplete_attempts_json.to_string(),
            written_at_ms,
        };
        record_cost_aggregate_rollup(connection, &record).await?;
    }
    Ok(())
}
