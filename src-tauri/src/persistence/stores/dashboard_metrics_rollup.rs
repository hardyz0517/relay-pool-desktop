use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::{
    models::dashboard_metrics::DashboardCostTotal,
    persistence::{
        error::PersistenceError,
        stores::{
            request_cost_write::RequestCostAggregateWrite,
            request_log_write::{RequestStartWrite, RequestTerminalWrite},
        },
    },
};

const MAX_COST_CURRENCIES_PER_ROW: usize = 32;
const MAX_CURRENCY_BYTES: usize = 16;
const ROLLUP_BUCKET_MS: i64 = 1_000;
const ROLLUP_KIND_SECOND: &str = "second";
const ROLLUP_KIND_LIFETIME: &str = "lifetime";

#[derive(Debug, Clone, Default)]
struct RequestMetricDelta {
    request_count: i64,
    terminal_count: i64,
    success_count: i64,
    failed_count: i64,
    interrupted_count: i64,
    in_progress_count: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    known_usage_request_count: i64,
    missing_usage_request_count: i64,
    stream_usage_missing_request_count: i64,
    not_applicable_usage_request_count: i64,
    unknown_usage_request_count: i64,
    total_duration_ms: i64,
    duration_sample_count: i64,
    first_token_total_ms: i64,
    first_token_sample_count: i64,
    invalid_duration_count: i64,
    unknown_lifecycle_count: i64,
}

impl RequestMetricDelta {
    fn has_negative(&self) -> bool {
        self.request_count < 0
            || self.terminal_count < 0
            || self.success_count < 0
            || self.failed_count < 0
            || self.interrupted_count < 0
            || self.in_progress_count < 0
            || self.prompt_tokens < 0
            || self.completion_tokens < 0
            || self.total_tokens < 0
            || self.known_usage_request_count < 0
            || self.missing_usage_request_count < 0
            || self.stream_usage_missing_request_count < 0
            || self.not_applicable_usage_request_count < 0
            || self.unknown_usage_request_count < 0
            || self.total_duration_ms < 0
            || self.duration_sample_count < 0
            || self.first_token_total_ms < 0
            || self.first_token_sample_count < 0
            || self.invalid_duration_count < 0
            || self.unknown_lifecycle_count < 0
    }
}

#[derive(Debug, Clone, Default)]
struct CostMetricDelta {
    legacy_or_missing_aggregate_count: i64,
    complete_single_currency_count: i64,
    complete_mixed_currency_count: i64,
    incomplete_count: i64,
    not_applicable_count: i64,
    no_attempts_count: i64,
    corrupt_cost_aggregate_count: i64,
    totals: Vec<DashboardCostTotal>,
}

impl CostMetricDelta {
    fn has_negative(&self) -> bool {
        self.legacy_or_missing_aggregate_count < 0
            || self.complete_single_currency_count < 0
            || self.complete_mixed_currency_count < 0
            || self.incomplete_count < 0
            || self.not_applicable_count < 0
            || self.no_attempts_count < 0
            || self.corrupt_cost_aggregate_count < 0
            || self.totals.iter().any(|total| total.amount_micro < 0)
    }
}

pub(crate) async fn clear_dashboard_metric_rollups(
    connection: &mut SqliteConnection,
) -> Result<(), PersistenceError> {
    sqlx::query("DELETE FROM dashboard_request_metric_rollups")
        .execute(&mut *connection)
        .await
        .map_err(PersistenceError::from)?;
    sqlx::query("DELETE FROM dashboard_request_cost_rollups")
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM dashboard_request_cost_totals_rollups")
        .execute(&mut *connection)
        .await?;
    Ok(())
}

pub(crate) async fn dashboard_rollups_rebuild_required(
    connection: &mut SqliteConnection,
) -> Result<bool, PersistenceError> {
    let request_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM request_logs WHERE received_at_ms > 0")
            .fetch_one(&mut *connection)
            .await
            .map_err(PersistenceError::from)?;
    if request_count == 0 {
        return Ok(false);
    }

    let rollup_request_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(request_count), 0)
         FROM dashboard_request_metric_rollups
         WHERE bucket_kind = 'lifetime'",
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(PersistenceError::from)?;
    Ok(rollup_request_count != request_count)
}

pub(crate) async fn rebuild_dashboard_metric_rollups(
    connection: &mut SqliteConnection,
) -> Result<(), PersistenceError> {
    clear_dashboard_metric_rollups(connection).await?;

    let request_rows = sqlx::query(
        r#"
        SELECT
            id,
            received_at_ms,
            terminal_at_ms,
            status,
            lifecycle_status,
            usage_status,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            first_token_ms
        FROM request_logs
        WHERE received_at_ms > 0
        ORDER BY received_at_ms ASC, id ASC
        "#,
    )
    .fetch_all(&mut *connection)
    .await?;

    for row in request_rows {
        let received_at_ms: i64 = row.try_get("received_at_ms")?;
        apply_request_metric_delta(connection, received_at_ms, &request_start_delta()).await?;
        apply_cost_metric_delta(
            connection,
            received_at_ms,
            &CostMetricDelta {
                legacy_or_missing_aggregate_count: 1,
                ..Default::default()
            },
        )
        .await?;

        if row.try_get::<Option<i64>, _>("terminal_at_ms")?.is_some() {
            let delta = request_finish_delta(
                row.try_get::<String, _>("status")?.as_str(),
                row.try_get::<Option<String>, _>("lifecycle_status")?
                    .as_deref(),
                row.try_get::<String, _>("usage_status")?.as_str(),
                row.try_get("prompt_tokens")?,
                row.try_get("completion_tokens")?,
                row.try_get("total_tokens")?,
                row.try_get("duration_ms")?,
                row.try_get("first_token_ms")?,
            );
            apply_request_metric_delta(connection, received_at_ms, &delta).await?;
        }
    }

    let aggregate_rows = sqlx::query(
        r#"
        SELECT
            a.request_id,
            a.status,
            a.totals_by_currency_json,
            a.compatibility_currency,
            a.compatibility_total_cost_micro,
            a.incomplete_attempts_json,
            a.created_at_ms,
            l.received_at_ms
        FROM routing_request_cost_aggregates a
        JOIN request_logs l ON l.id = a.request_id
        WHERE l.received_at_ms > 0
        ORDER BY l.received_at_ms ASC, a.request_id ASC
        "#,
    )
    .fetch_all(&mut *connection)
    .await?;

    for row in aggregate_rows {
        let received_at_ms: i64 = row.try_get("received_at_ms")?;
        let record = RequestCostAggregateWrite {
            request_id: row.try_get("request_id")?,
            status: row.try_get("status")?,
            totals_by_currency_json: row.try_get("totals_by_currency_json")?,
            compatibility_currency: row.try_get("compatibility_currency")?,
            compatibility_total_cost_micro: row.try_get("compatibility_total_cost_micro")?,
            incomplete_attempts_json: row.try_get("incomplete_attempts_json")?,
            written_at_ms: row.try_get("created_at_ms")?,
        };
        apply_cost_metric_delta(
            connection,
            received_at_ms,
            &CostMetricDelta {
                legacy_or_missing_aggregate_count: -1,
                ..Default::default()
            },
        )
        .await?;
        apply_cost_metric_delta(
            connection,
            received_at_ms,
            &cost_delta_from_record(&record)?,
        )
        .await?;
    }

    Ok(())
}

pub(crate) async fn record_request_start_rollup(
    connection: &mut SqliteConnection,
    record: &RequestStartWrite,
) -> Result<(), PersistenceError> {
    apply_request_metric_delta(connection, record.received_at_ms, &request_start_delta()).await?;
    apply_cost_metric_delta(
        connection,
        record.received_at_ms,
        &CostMetricDelta {
            legacy_or_missing_aggregate_count: 1,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

pub(crate) async fn record_request_finish_rollup(
    connection: &mut SqliteConnection,
    record: &RequestTerminalWrite,
) -> Result<(), PersistenceError> {
    if dashboard_rollups_rebuild_required(connection).await? {
        rebuild_dashboard_metric_rollups(connection).await?;
        return Ok(());
    }
    let duration_ms = Some((record.terminal_at_ms - record.received_at_ms).max(0));
    let delta = request_finish_delta(
        &record.status,
        Some(&record.lifecycle_status),
        &record.usage_status,
        record.annotations.prompt_tokens,
        record.annotations.completion_tokens,
        record.annotations.total_tokens,
        duration_ms,
        record.annotations.first_token_ms,
    );
    apply_request_metric_delta(connection, record.received_at_ms, &delta).await
}

pub(crate) async fn record_cost_aggregate_rollup(
    connection: &mut SqliteConnection,
    record: &RequestCostAggregateWrite,
) -> Result<(), PersistenceError> {
    if dashboard_rollups_rebuild_required(connection).await? {
        rebuild_dashboard_metric_rollups(connection).await?;
        return Ok(());
    }
    let received_at_ms: i64 =
        sqlx::query_scalar("SELECT received_at_ms FROM request_logs WHERE id = ?")
            .bind(&record.request_id)
            .fetch_one(&mut *connection)
            .await?;
    if received_at_ms <= 0 {
        return Ok(());
    }
    apply_cost_metric_delta(
        connection,
        received_at_ms,
        &CostMetricDelta {
            legacy_or_missing_aggregate_count: -1,
            ..Default::default()
        },
    )
    .await?;
    apply_cost_metric_delta(connection, received_at_ms, &cost_delta_from_record(record)?).await
}

fn request_start_delta() -> RequestMetricDelta {
    RequestMetricDelta {
        request_count: 1,
        in_progress_count: 1,
        ..Default::default()
    }
}

fn request_finish_delta(
    status: &str,
    lifecycle_status: Option<&str>,
    usage_status: &str,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    duration_ms: Option<i64>,
    first_token_ms: Option<i64>,
) -> RequestMetricDelta {
    let known_usage = usage_status == "complete" && total_tokens.is_some();
    let missing_usage = matches!(usage_status, "missing_usage" | "stream_usage_missing");
    let duration_sample = duration_ms.filter(|value| *value >= 0);
    let first_token_sample = first_token_ms.filter(|value| *value >= 0);
    RequestMetricDelta {
        terminal_count: 1,
        success_count: i64::from(status == "success"),
        failed_count: i64::from(status == "failed"),
        interrupted_count: i64::from(status == "interrupted"),
        in_progress_count: -1,
        prompt_tokens: if known_usage {
            prompt_tokens.unwrap_or_default().max(0)
        } else {
            0
        },
        completion_tokens: if known_usage {
            completion_tokens.unwrap_or_default().max(0)
        } else {
            0
        },
        total_tokens: if known_usage {
            total_tokens.unwrap_or_default().max(0)
        } else {
            0
        },
        known_usage_request_count: i64::from(known_usage),
        missing_usage_request_count: i64::from(missing_usage),
        stream_usage_missing_request_count: i64::from(usage_status == "stream_usage_missing"),
        not_applicable_usage_request_count: i64::from(usage_status == "not_applicable"),
        unknown_usage_request_count: i64::from(usage_status == "unknown_legacy"),
        total_duration_ms: duration_sample.unwrap_or_default(),
        duration_sample_count: i64::from(duration_sample.is_some()),
        first_token_total_ms: first_token_sample.unwrap_or_default(),
        first_token_sample_count: i64::from(first_token_sample.is_some()),
        invalid_duration_count: i64::from(duration_sample.is_none()),
        unknown_lifecycle_count: i64::from(!matches!(
            lifecycle_status,
            Some("admitted" | "completed" | "partial_success" | "failed" | "interrupted")
        )),
        ..Default::default()
    }
}

async fn apply_request_metric_delta(
    connection: &mut SqliteConnection,
    received_at_ms: i64,
    delta: &RequestMetricDelta,
) -> Result<(), PersistenceError> {
    if received_at_ms <= 0 {
        return Ok(());
    }
    for &(bucket_kind, bucket_start_ms) in [
        (ROLLUP_KIND_SECOND, bucket_floor_ms(received_at_ms)),
        (ROLLUP_KIND_LIFETIME, 0),
    ]
    .iter()
    {
        if delta.has_negative() {
            let updated = sqlx::query(
                r#"
                UPDATE dashboard_request_metric_rollups SET
                    request_count = request_count + ?,
                    terminal_count = terminal_count + ?,
                    success_count = success_count + ?,
                    failed_count = failed_count + ?,
                    interrupted_count = interrupted_count + ?,
                    in_progress_count = in_progress_count + ?,
                    prompt_tokens = prompt_tokens + ?,
                    completion_tokens = completion_tokens + ?,
                    total_tokens = total_tokens + ?,
                    known_usage_request_count = known_usage_request_count + ?,
                    missing_usage_request_count = missing_usage_request_count + ?,
                    stream_usage_missing_request_count = stream_usage_missing_request_count + ?,
                    not_applicable_usage_request_count = not_applicable_usage_request_count + ?,
                    unknown_usage_request_count = unknown_usage_request_count + ?,
                    total_duration_ms = total_duration_ms + ?,
                    invalid_duration_count = invalid_duration_count + ?,
                    duration_sample_count = duration_sample_count + ?,
                    first_token_total_ms = first_token_total_ms + ?,
                    first_token_sample_count = first_token_sample_count + ?,
                    unknown_lifecycle_count = unknown_lifecycle_count + ?
                WHERE bucket_kind = ? AND bucket_start_ms = ?
                "#,
            )
            .bind(delta.request_count)
            .bind(delta.terminal_count)
            .bind(delta.success_count)
            .bind(delta.failed_count)
            .bind(delta.interrupted_count)
            .bind(delta.in_progress_count)
            .bind(delta.prompt_tokens)
            .bind(delta.completion_tokens)
            .bind(delta.total_tokens)
            .bind(delta.known_usage_request_count)
            .bind(delta.missing_usage_request_count)
            .bind(delta.stream_usage_missing_request_count)
            .bind(delta.not_applicable_usage_request_count)
            .bind(delta.unknown_usage_request_count)
            .bind(delta.total_duration_ms)
            .bind(delta.invalid_duration_count)
            .bind(delta.duration_sample_count)
            .bind(delta.first_token_total_ms)
            .bind(delta.first_token_sample_count)
            .bind(delta.unknown_lifecycle_count)
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(PersistenceError::InvariantViolation(
                    "dashboard request metric rollup delta missing base row".to_string(),
                ));
            }
        } else {
            sqlx::query(
                r#"
                INSERT INTO dashboard_request_metric_rollups (
                    bucket_kind, bucket_start_ms, request_count, terminal_count, success_count,
                    failed_count, interrupted_count, in_progress_count, prompt_tokens,
                    completion_tokens, total_tokens, known_usage_request_count,
                    missing_usage_request_count, stream_usage_missing_request_count,
                    not_applicable_usage_request_count, unknown_usage_request_count,
                    total_duration_ms, invalid_duration_count, duration_sample_count,
                    first_token_total_ms, first_token_sample_count, unknown_lifecycle_count
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(bucket_kind, bucket_start_ms) DO UPDATE SET
                    request_count = request_count + excluded.request_count,
                    terminal_count = terminal_count + excluded.terminal_count,
                    success_count = success_count + excluded.success_count,
                    failed_count = failed_count + excluded.failed_count,
                    interrupted_count = interrupted_count + excluded.interrupted_count,
                    in_progress_count = in_progress_count + excluded.in_progress_count,
                    prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                    completion_tokens = completion_tokens + excluded.completion_tokens,
                    total_tokens = total_tokens + excluded.total_tokens,
                    known_usage_request_count = known_usage_request_count + excluded.known_usage_request_count,
                    missing_usage_request_count = missing_usage_request_count + excluded.missing_usage_request_count,
                    stream_usage_missing_request_count = stream_usage_missing_request_count + excluded.stream_usage_missing_request_count,
                    not_applicable_usage_request_count = not_applicable_usage_request_count + excluded.not_applicable_usage_request_count,
                    unknown_usage_request_count = unknown_usage_request_count + excluded.unknown_usage_request_count,
                    total_duration_ms = total_duration_ms + excluded.total_duration_ms,
                    invalid_duration_count = invalid_duration_count + excluded.invalid_duration_count,
                    duration_sample_count = duration_sample_count + excluded.duration_sample_count,
                    first_token_total_ms = first_token_total_ms + excluded.first_token_total_ms,
                    first_token_sample_count = first_token_sample_count + excluded.first_token_sample_count,
                    unknown_lifecycle_count = unknown_lifecycle_count + excluded.unknown_lifecycle_count
                "#,
            )
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .bind(delta.request_count)
            .bind(delta.terminal_count)
            .bind(delta.success_count)
            .bind(delta.failed_count)
            .bind(delta.interrupted_count)
            .bind(delta.in_progress_count)
            .bind(delta.prompt_tokens)
            .bind(delta.completion_tokens)
            .bind(delta.total_tokens)
            .bind(delta.known_usage_request_count)
            .bind(delta.missing_usage_request_count)
            .bind(delta.stream_usage_missing_request_count)
            .bind(delta.not_applicable_usage_request_count)
            .bind(delta.unknown_usage_request_count)
            .bind(delta.total_duration_ms)
            .bind(delta.invalid_duration_count)
            .bind(delta.duration_sample_count)
            .bind(delta.first_token_total_ms)
            .bind(delta.first_token_sample_count)
            .bind(delta.unknown_lifecycle_count)
            .execute(&mut *connection)
            .await?;
        }
    }
    Ok(())
}

async fn apply_cost_metric_delta(
    connection: &mut SqliteConnection,
    received_at_ms: i64,
    delta: &CostMetricDelta,
) -> Result<(), PersistenceError> {
    if received_at_ms <= 0 {
        return Ok(());
    }
    for &(bucket_kind, bucket_start_ms) in [
        (ROLLUP_KIND_SECOND, bucket_floor_ms(received_at_ms)),
        (ROLLUP_KIND_LIFETIME, 0),
    ]
    .iter()
    {
        if delta.has_negative() {
            let updated = sqlx::query(
                r#"
                UPDATE dashboard_request_cost_rollups SET
                    legacy_or_missing_aggregate_count = legacy_or_missing_aggregate_count + ?,
                    complete_single_currency_count = complete_single_currency_count + ?,
                    complete_mixed_currency_count = complete_mixed_currency_count + ?,
                    incomplete_count = incomplete_count + ?,
                    not_applicable_count = not_applicable_count + ?,
                    no_attempts_count = no_attempts_count + ?,
                    corrupt_cost_aggregate_count = corrupt_cost_aggregate_count + ?
                WHERE bucket_kind = ? AND bucket_start_ms = ?
                "#,
            )
            .bind(delta.legacy_or_missing_aggregate_count)
            .bind(delta.complete_single_currency_count)
            .bind(delta.complete_mixed_currency_count)
            .bind(delta.incomplete_count)
            .bind(delta.not_applicable_count)
            .bind(delta.no_attempts_count)
            .bind(delta.corrupt_cost_aggregate_count)
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(PersistenceError::InvariantViolation(
                    "dashboard cost rollup delta missing base row".to_string(),
                ));
            }
        } else {
            sqlx::query(
                r#"
                INSERT INTO dashboard_request_cost_rollups (
                    bucket_kind, bucket_start_ms, legacy_or_missing_aggregate_count,
                    complete_single_currency_count, complete_mixed_currency_count,
                    incomplete_count, not_applicable_count, no_attempts_count,
                    corrupt_cost_aggregate_count
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(bucket_kind, bucket_start_ms) DO UPDATE SET
                    legacy_or_missing_aggregate_count = legacy_or_missing_aggregate_count + excluded.legacy_or_missing_aggregate_count,
                    complete_single_currency_count = complete_single_currency_count + excluded.complete_single_currency_count,
                    complete_mixed_currency_count = complete_mixed_currency_count + excluded.complete_mixed_currency_count,
                    incomplete_count = incomplete_count + excluded.incomplete_count,
                    not_applicable_count = not_applicable_count + excluded.not_applicable_count,
                    no_attempts_count = no_attempts_count + excluded.no_attempts_count,
                    corrupt_cost_aggregate_count = corrupt_cost_aggregate_count + excluded.corrupt_cost_aggregate_count
                "#,
            )
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .bind(delta.legacy_or_missing_aggregate_count)
            .bind(delta.complete_single_currency_count)
            .bind(delta.complete_mixed_currency_count)
            .bind(delta.incomplete_count)
            .bind(delta.not_applicable_count)
            .bind(delta.no_attempts_count)
            .bind(delta.corrupt_cost_aggregate_count)
            .execute(&mut *connection)
            .await?;
        }
        for total in &delta.totals {
            sqlx::query(
                r#"
                INSERT INTO dashboard_request_cost_totals_rollups (
                    bucket_kind, bucket_start_ms, currency, amount_micro, request_count
                ) VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(bucket_kind, bucket_start_ms, currency) DO UPDATE SET
                    amount_micro = amount_micro + excluded.amount_micro,
                    request_count = request_count + excluded.request_count
                "#,
            )
            .bind(bucket_kind)
            .bind(bucket_start_ms)
            .bind(&total.currency)
            .bind(total.amount_micro)
            .bind(total.request_count as i64)
            .execute(&mut *connection)
            .await?;
        }
    }
    Ok(())
}

fn bucket_floor_ms(value: i64) -> i64 {
    value.div_euclid(ROLLUP_BUCKET_MS) * ROLLUP_BUCKET_MS
}

fn cost_delta_from_record(
    record: &RequestCostAggregateWrite,
) -> Result<CostMetricDelta, PersistenceError> {
    let mut delta = CostMetricDelta {
        complete_single_currency_count: if record.status == "complete_single_currency" {
            1
        } else {
            0
        },
        complete_mixed_currency_count: if record.status == "complete_mixed_currency" {
            1
        } else {
            0
        },
        incomplete_count: if record.status == "incomplete" { 1 } else { 0 },
        not_applicable_count: if record.status == "not_applicable" {
            1
        } else {
            0
        },
        no_attempts_count: if record.status == "no_attempts" { 1 } else { 0 },
        ..Default::default()
    };

    match record.status.as_str() {
        "complete_single_currency" => {
            let Some(currency) = record.compatibility_currency.as_ref() else {
                delta.corrupt_cost_aggregate_count = 1;
                return Ok(delta);
            };
            let Some(amount_micro) = record.compatibility_total_cost_micro else {
                delta.corrupt_cost_aggregate_count = 1;
                return Ok(delta);
            };
            let normalized = currency.trim().to_ascii_uppercase();
            if normalized.len() < 3
                || normalized.len() > MAX_CURRENCY_BYTES
                || !normalized.bytes().all(|byte| byte.is_ascii_uppercase())
                || amount_micro < 0
            {
                delta.corrupt_cost_aggregate_count = 1;
                return Ok(delta);
            }
            delta.totals.push(DashboardCostTotal {
                currency: normalized,
                amount_micro,
                request_count: 1,
            });
        }
        "complete_mixed_currency" | "incomplete" | "not_applicable" | "no_attempts" => {
            let value: Value = match serde_json::from_str(record.totals_by_currency_json.trim()) {
                Ok(value) => value,
                Err(_) => {
                    delta.corrupt_cost_aggregate_count = 1;
                    return Ok(delta);
                }
            };
            let Some(object) = value.as_object() else {
                delta.corrupt_cost_aggregate_count = 1;
                return Ok(delta);
            };
            if object.len() > MAX_COST_CURRENCIES_PER_ROW {
                delta.corrupt_cost_aggregate_count = 1;
                return Ok(delta);
            }
            for (currency, amount) in object {
                let normalized = currency.trim().to_ascii_uppercase();
                let valid_currency = normalized.len() >= 3
                    && normalized.len() <= MAX_CURRENCY_BYTES
                    && normalized.bytes().all(|byte| byte.is_ascii_uppercase());
                let Some(amount_micro) = amount.as_i64().filter(|amount| *amount >= 0) else {
                    delta.corrupt_cost_aggregate_count += 1;
                    continue;
                };
                if valid_currency {
                    delta.totals.push(DashboardCostTotal {
                        currency: normalized,
                        amount_micro,
                        request_count: 1,
                    });
                } else {
                    delta.corrupt_cost_aggregate_count += 1;
                }
            }
        }
        _ => {
            delta.corrupt_cost_aggregate_count = 1;
        }
    }

    Ok(delta)
}
