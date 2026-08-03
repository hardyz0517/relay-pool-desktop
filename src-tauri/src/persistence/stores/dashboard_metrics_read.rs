use std::collections::BTreeMap;

use sqlx::{Row, SqliteConnection};

use crate::{
    models::dashboard_metrics::{
        DashboardCostMetrics, DashboardCostTotal, DashboardLiveMetricsDataQuality,
        DashboardMetricsDataQuality, DashboardPeriodMetrics,
    },
    persistence::{error::PersistenceError, read_session::ReadSession},
};

const MAX_COST_CURRENCIES_PER_ROW: usize = 32;
const MAX_CURRENCY_BYTES: usize = 16;
const ROLLUP_BUCKET_MS: i64 = 1_000;
const ROLLUP_KIND_SECOND: &str = "second";
const ROLLUP_KIND_LIFETIME: &str = "lifetime";

#[derive(Debug, Clone, Default)]
pub(crate) struct DashboardLiveReadResult {
    pub recent: DashboardPeriodMetrics,
    pub today: DashboardPeriodMetrics,
    pub today_costs: DashboardCostMetrics,
    pub data_quality: DashboardLiveMetricsDataQuality,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DashboardCumulativeReadResult {
    pub lifetime: DashboardPeriodMetrics,
    pub lifetime_costs: DashboardCostMetrics,
    pub data_quality: DashboardMetricsDataQuality,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DashboardMetricsReadRepository;

impl DashboardMetricsReadRepository {
    pub(crate) async fn load_live(
        &self,
        read: &mut ReadSession,
        recent_start_ms: i64,
        captured_at_ms: i64,
        day_start_ms: i64,
    ) -> Result<DashboardLiveReadResult, PersistenceError> {
        let recent = load_period_window(read.connection(), recent_start_ms, captured_at_ms).await?;
        let today = load_period_window(read.connection(), day_start_ms, captured_at_ms).await?;
        let today_costs =
            load_costs_window(read.connection(), day_start_ms, captured_at_ms).await?;
        let corrupt_cost_aggregate_count = today_costs.corrupt_cost_aggregate_count;
        let invalid_duration_count = today.invalid_duration_count;
        let unknown_lifecycle_count = today.unknown_lifecycle_count;
        let today_period = today.period;
        let recent_period = recent.period;
        Ok(DashboardLiveReadResult {
            recent: recent_period,
            today: today_period,
            today_costs: today_costs.metrics,
            data_quality: DashboardLiveMetricsDataQuality {
                invalid_duration_count,
                unknown_lifecycle_count,
                corrupt_cost_aggregate_count,
            },
        })
    }

    pub(crate) async fn load_cumulative(
        &self,
        read: &mut ReadSession,
        captured_at_ms: i64,
    ) -> Result<DashboardCumulativeReadResult, PersistenceError> {
        let lifetime = load_lifetime_period(read.connection()).await?;
        let mut lifetime_costs = load_lifetime_costs(read.connection()).await?;
        let mut corrupt_cost_aggregate_count = lifetime_costs.corrupt_cost_aggregate_count;
        let invalid_timestamp_count = scalar_count(
            read.connection(),
            "SELECT COUNT(*) FROM request_logs WHERE received_at_ms IS NULL OR received_at_ms <= 0",
        )
        .await?;
        let future_timestamp_count = scalar_count_bind(
            read.connection(),
            "SELECT COUNT(*) FROM request_logs WHERE received_at_ms >= ?",
            captured_at_ms,
        )
        .await?;
        let mut lifetime_period = lifetime.period;
        let mut invalid_duration_count = lifetime.invalid_duration_count;
        let mut unknown_lifecycle_count = lifetime.unknown_lifecycle_count;
        if future_timestamp_count > 0 {
            let future_period =
                load_period_raw(read.connection(), captured_at_ms, i64::MAX).await?;
            let future_costs = load_costs_raw(read.connection(), captured_at_ms, i64::MAX).await?;
            invalid_duration_count = invalid_duration_count
                .checked_sub(future_period.invalid_duration_count)
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation("dashboard period underflow".to_string())
                })?;
            unknown_lifecycle_count = unknown_lifecycle_count
                .checked_sub(future_period.unknown_lifecycle_count)
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation("dashboard period underflow".to_string())
                })?;
            corrupt_cost_aggregate_count = corrupt_cost_aggregate_count
                .checked_sub(future_costs.corrupt_cost_aggregate_count)
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
                })?;
            subtract_period_metrics(&mut lifetime_period, future_period.period)?;
            subtract_cost_metrics(&mut lifetime_costs.metrics, future_costs.metrics)?;
        }
        lifetime_costs.metrics.cost_totals_complete = lifetime_costs.metrics.incomplete_count == 0
            && lifetime_costs.metrics.legacy_or_missing_aggregate_count == 0
            && corrupt_cost_aggregate_count == 0;
        lifetime_period.finish_averages();
        Ok(DashboardCumulativeReadResult {
            lifetime: lifetime_period,
            lifetime_costs: lifetime_costs.metrics,
            data_quality: DashboardMetricsDataQuality {
                invalid_timestamp_count,
                future_timestamp_count,
                invalid_duration_count,
                unknown_lifecycle_count,
                corrupt_cost_aggregate_count,
            },
        })
    }
}

#[derive(Debug, Clone, Default)]
struct RawPeriod {
    period: DashboardPeriodMetrics,
    invalid_duration_count: u64,
    unknown_lifecycle_count: u64,
}

#[derive(Debug, Clone, Default)]
struct RawCostMetrics {
    metrics: DashboardCostMetrics,
    corrupt_cost_aggregate_count: u64,
}

async fn load_period_window(
    connection: &mut SqliteConnection,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawPeriod, PersistenceError> {
    let mut total = RawPeriod::default();
    let full_start_ms = bucket_ceil_ms(start_ms);
    let full_end_ms = bucket_floor_ms(end_ms);

    if full_start_ms < full_end_ms {
        add_period_metrics(
            &mut total,
            load_period_rollup(connection, ROLLUP_KIND_SECOND, full_start_ms, full_end_ms).await?,
        )?;
    }
    if start_ms < full_start_ms {
        add_period_metrics(
            &mut total,
            load_period_raw(connection, start_ms, full_start_ms).await?,
        )?;
    }
    if full_end_ms < end_ms {
        add_period_metrics(
            &mut total,
            load_period_raw(connection, full_end_ms, end_ms).await?,
        )?;
    }
    total.period.finish_averages();
    Ok(total)
}

async fn load_lifetime_period(
    connection: &mut SqliteConnection,
) -> Result<RawPeriod, PersistenceError> {
    let mut period =
        load_period_rollup(connection, ROLLUP_KIND_LIFETIME, 0, ROLLUP_BUCKET_MS).await?;
    period.period.finish_averages();
    Ok(period)
}

async fn load_period_rollup(
    connection: &mut SqliteConnection,
    bucket_kind: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawPeriod, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE(SUM(request_count), 0) AS request_count,
            COALESCE(SUM(terminal_count), 0) AS terminal_count,
            COALESCE(SUM(success_count), 0) AS success_count,
            COALESCE(SUM(failed_count), 0) AS failed_count,
            COALESCE(SUM(interrupted_count), 0) AS interrupted_count,
            COALESCE(SUM(in_progress_count), 0) AS in_progress_count,
            COALESCE(SUM(prompt_tokens), 0) AS prompt_tokens,
            COALESCE(SUM(completion_tokens), 0) AS completion_tokens,
            COALESCE(SUM(total_tokens), 0) AS total_tokens,
            COALESCE(SUM(known_usage_request_count), 0) AS known_usage_request_count,
            COALESCE(SUM(missing_usage_request_count), 0) AS missing_usage_request_count,
            COALESCE(SUM(stream_usage_missing_request_count), 0) AS stream_usage_missing_request_count,
            COALESCE(SUM(not_applicable_usage_request_count), 0) AS not_applicable_usage_request_count,
            COALESCE(SUM(unknown_usage_request_count), 0) AS unknown_usage_request_count,
            COALESCE(SUM(total_duration_ms), 0) AS total_duration_ms,
            COALESCE(SUM(invalid_duration_count), 0) AS invalid_duration_count,
            COALESCE(SUM(duration_sample_count), 0) AS duration_sample_count,
            COALESCE(SUM(first_token_total_ms), 0) AS first_token_total_ms,
            COALESCE(SUM(first_token_sample_count), 0) AS first_token_sample_count,
            COALESCE(SUM(unknown_lifecycle_count), 0) AS unknown_lifecycle_count
        FROM dashboard_request_metric_rollups
        WHERE bucket_kind = ? AND bucket_start_ms >= ? AND bucket_start_ms < ?
        "#,
    )
    .bind(bucket_kind)
    .bind(start_ms)
    .bind(end_ms)
    .fetch_one(&mut *connection)
    .await?;

    Ok(RawPeriod {
        period: DashboardPeriodMetrics {
            request_count: non_negative(row.get("request_count"))?,
            terminal_count: non_negative(row.get("terminal_count"))?,
            success_count: non_negative(row.get("success_count"))?,
            failed_count: non_negative(row.get("failed_count"))?,
            interrupted_count: non_negative(row.get("interrupted_count"))?,
            in_progress_count: non_negative(row.get("in_progress_count"))?,
            prompt_tokens: non_negative(row.get("prompt_tokens"))?,
            completion_tokens: non_negative(row.get("completion_tokens"))?,
            total_tokens: non_negative(row.get("total_tokens"))?,
            known_usage_request_count: non_negative(row.get("known_usage_request_count"))?,
            missing_usage_request_count: non_negative(row.get("missing_usage_request_count"))?,
            stream_usage_missing_request_count: non_negative(
                row.get("stream_usage_missing_request_count"),
            )?,
            not_applicable_usage_request_count: non_negative(
                row.get("not_applicable_usage_request_count"),
            )?,
            unknown_usage_request_count: non_negative(row.get("unknown_usage_request_count"))?,
            total_duration_ms: non_negative(row.get("total_duration_ms"))?,
            duration_sample_count: non_negative(row.get("duration_sample_count"))?,
            first_token_total_ms: non_negative(row.get("first_token_total_ms"))?,
            first_token_sample_count: non_negative(row.get("first_token_sample_count"))?,
            ..Default::default()
        },
        invalid_duration_count: non_negative(row.get("invalid_duration_count"))?,
        unknown_lifecycle_count: non_negative(row.get("unknown_lifecycle_count"))?,
    })
}

async fn load_period_raw(
    connection: &mut SqliteConnection,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawPeriod, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) AS request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL THEN 1 ELSE 0 END), 0) AS terminal_count,
            COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS success_count,
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS failed_count,
            COALESCE(SUM(CASE WHEN status = 'interrupted' THEN 1 ELSE 0 END), 0) AS interrupted_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NULL OR usage_status = 'in_progress' THEN 1 ELSE 0 END), 0) AS in_progress_count,
            COALESCE(SUM(CASE WHEN usage_status = 'complete' AND prompt_tokens IS NOT NULL THEN prompt_tokens ELSE 0 END), 0) AS prompt_tokens,
            COALESCE(SUM(CASE WHEN usage_status = 'complete' AND completion_tokens IS NOT NULL THEN completion_tokens ELSE 0 END), 0) AS completion_tokens,
            COALESCE(SUM(CASE WHEN usage_status = 'complete' AND total_tokens IS NOT NULL THEN total_tokens ELSE 0 END), 0) AS total_tokens,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND usage_status = 'complete' AND total_tokens IS NOT NULL THEN 1 ELSE 0 END), 0) AS known_usage_request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND usage_status IN ('missing_usage', 'stream_usage_missing') THEN 1 ELSE 0 END), 0) AS missing_usage_request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND usage_status = 'stream_usage_missing' THEN 1 ELSE 0 END), 0) AS stream_usage_missing_request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND usage_status = 'not_applicable' THEN 1 ELSE 0 END), 0) AS not_applicable_usage_request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND usage_status = 'unknown_legacy' THEN 1 ELSE 0 END), 0) AS unknown_usage_request_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND duration_ms >= 0 THEN duration_ms ELSE 0 END), 0) AS total_duration_ms,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND (duration_ms IS NULL OR duration_ms < 0) THEN 1 ELSE 0 END), 0) AS invalid_duration_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND duration_ms >= 0 THEN 1 ELSE 0 END), 0) AS duration_sample_count,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND first_token_ms >= 0 THEN first_token_ms ELSE 0 END), 0) AS first_token_total_ms,
            COALESCE(SUM(CASE WHEN terminal_at_ms IS NOT NULL AND first_token_ms >= 0 THEN 1 ELSE 0 END), 0) AS first_token_sample_count,
            COALESCE(SUM(CASE WHEN lifecycle_status IS NULL OR lifecycle_status NOT IN ('admitted', 'completed', 'partial_success', 'failed', 'interrupted') THEN 1 ELSE 0 END), 0) AS unknown_lifecycle_count
        FROM request_logs
        WHERE received_at_ms >= ? AND received_at_ms < ?
        "#,
    )
    .bind(start_ms)
    .bind(end_ms)
    .fetch_one(&mut *connection)
    .await?;

    let mut period = DashboardPeriodMetrics {
        request_count: non_negative(row.get("request_count"))?,
        terminal_count: non_negative(row.get("terminal_count"))?,
        success_count: non_negative(row.get("success_count"))?,
        failed_count: non_negative(row.get("failed_count"))?,
        interrupted_count: non_negative(row.get("interrupted_count"))?,
        in_progress_count: non_negative(row.get("in_progress_count"))?,
        prompt_tokens: non_negative(row.get("prompt_tokens"))?,
        completion_tokens: non_negative(row.get("completion_tokens"))?,
        total_tokens: non_negative(row.get("total_tokens"))?,
        known_usage_request_count: non_negative(row.get("known_usage_request_count"))?,
        missing_usage_request_count: non_negative(row.get("missing_usage_request_count"))?,
        stream_usage_missing_request_count: non_negative(
            row.get("stream_usage_missing_request_count"),
        )?,
        not_applicable_usage_request_count: non_negative(
            row.get("not_applicable_usage_request_count"),
        )?,
        unknown_usage_request_count: non_negative(row.get("unknown_usage_request_count"))?,
        total_duration_ms: non_negative(row.get("total_duration_ms"))?,
        duration_sample_count: non_negative(row.get("duration_sample_count"))?,
        first_token_total_ms: non_negative(row.get("first_token_total_ms"))?,
        first_token_sample_count: non_negative(row.get("first_token_sample_count"))?,
        ..Default::default()
    };
    period.finish_averages();
    Ok(RawPeriod {
        period,
        invalid_duration_count: non_negative(row.get("invalid_duration_count"))?,
        unknown_lifecycle_count: non_negative(row.get("unknown_lifecycle_count"))?,
    })
}

async fn load_costs_window(
    connection: &mut SqliteConnection,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawCostMetrics, PersistenceError> {
    let mut total = RawCostMetrics::default();
    let full_start_ms = bucket_ceil_ms(start_ms);
    let full_end_ms = bucket_floor_ms(end_ms);

    if full_start_ms < full_end_ms {
        add_cost_metrics(
            &mut total,
            load_costs_rollup(connection, ROLLUP_KIND_SECOND, full_start_ms, full_end_ms).await?,
        )?;
    }
    if start_ms < full_start_ms {
        add_cost_metrics(
            &mut total,
            load_costs_raw(connection, start_ms, full_start_ms).await?,
        )?;
    }
    if full_end_ms < end_ms {
        add_cost_metrics(
            &mut total,
            load_costs_raw(connection, full_end_ms, end_ms).await?,
        )?;
    }
    Ok(total)
}

async fn load_lifetime_costs(
    connection: &mut SqliteConnection,
) -> Result<RawCostMetrics, PersistenceError> {
    load_costs_rollup(connection, ROLLUP_KIND_LIFETIME, 0, ROLLUP_BUCKET_MS).await
}

async fn load_costs_rollup(
    connection: &mut SqliteConnection,
    bucket_kind: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawCostMetrics, PersistenceError> {
    let counts = sqlx::query(
        r#"
        SELECT
            COALESCE(SUM(legacy_or_missing_aggregate_count), 0) AS legacy_or_missing_aggregate_count,
            COALESCE(SUM(complete_single_currency_count), 0) AS complete_single_currency_count,
            COALESCE(SUM(complete_mixed_currency_count), 0) AS complete_mixed_currency_count,
            COALESCE(SUM(incomplete_count), 0) AS incomplete_count,
            COALESCE(SUM(not_applicable_count), 0) AS not_applicable_count,
            COALESCE(SUM(no_attempts_count), 0) AS no_attempts_count,
            COALESCE(SUM(corrupt_cost_aggregate_count), 0) AS corrupt_cost_aggregate_count
        FROM dashboard_request_cost_rollups
        WHERE bucket_kind = ? AND bucket_start_ms >= ? AND bucket_start_ms < ?
        "#,
    )
    .bind(bucket_kind)
    .bind(start_ms)
    .bind(end_ms)
    .fetch_one(&mut *connection)
    .await?;

    let single_currency_rows = sqlx::query(
        r#"
        SELECT
            currency,
            COALESCE(SUM(amount_micro), 0) AS amount_micro,
            COALESCE(SUM(request_count), 0) AS request_count
        FROM dashboard_request_cost_totals_rollups
        WHERE bucket_kind = ? AND bucket_start_ms >= ? AND bucket_start_ms < ?
        GROUP BY currency
        ORDER BY currency ASC
        "#,
    )
    .bind(bucket_kind)
    .bind(start_ms)
    .bind(end_ms)
    .fetch_all(&mut *connection)
    .await?;

    let mut metrics = DashboardCostMetrics {
        legacy_or_missing_aggregate_count: non_negative(
            counts.get("legacy_or_missing_aggregate_count"),
        )?,
        complete_single_currency_count: non_negative(counts.get("complete_single_currency_count"))?,
        complete_mixed_currency_count: non_negative(counts.get("complete_mixed_currency_count"))?,
        incomplete_count: non_negative(counts.get("incomplete_count"))?,
        not_applicable_count: non_negative(counts.get("not_applicable_count"))?,
        no_attempts_count: non_negative(counts.get("no_attempts_count"))?,
        ..Default::default()
    };
    for row in single_currency_rows {
        metrics.totals.push(DashboardCostTotal {
            currency: row.try_get("currency")?,
            amount_micro: row.try_get("amount_micro")?,
            request_count: non_negative(row.try_get("request_count")?)?,
        });
    }
    metrics
        .totals
        .sort_by(|left, right| left.currency.cmp(&right.currency));
    let corrupt = non_negative(counts.get("corrupt_cost_aggregate_count"))?;
    metrics.cost_totals_complete = metrics.incomplete_count == 0
        && metrics.legacy_or_missing_aggregate_count == 0
        && corrupt == 0;
    Ok(RawCostMetrics {
        metrics,
        corrupt_cost_aggregate_count: corrupt,
    })
}

async fn load_costs_raw(
    connection: &mut SqliteConnection,
    start_ms: i64,
    end_ms: i64,
) -> Result<RawCostMetrics, PersistenceError> {
    let counts = sqlx::query(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN a.request_id IS NULL THEN 1 ELSE 0 END), 0) AS legacy_or_missing_aggregate_count,
            COALESCE(SUM(CASE WHEN a.status = 'complete_single_currency' THEN 1 ELSE 0 END), 0) AS complete_single_currency_count,
            COALESCE(SUM(CASE WHEN a.status = 'complete_mixed_currency' THEN 1 ELSE 0 END), 0) AS complete_mixed_currency_count,
            COALESCE(SUM(CASE WHEN a.status = 'incomplete' THEN 1 ELSE 0 END), 0) AS incomplete_count,
            COALESCE(SUM(CASE WHEN a.status = 'not_applicable' THEN 1 ELSE 0 END), 0) AS not_applicable_count,
            COALESCE(SUM(CASE WHEN a.status = 'no_attempts' THEN 1 ELSE 0 END), 0) AS no_attempts_count,
            COALESCE(SUM(CASE
                WHEN a.request_id IS NOT NULL
                 AND a.status NOT IN (
                    'complete_single_currency',
                    'complete_mixed_currency',
                    'incomplete',
                    'not_applicable',
                    'no_attempts'
                 )
                THEN 1 ELSE 0
            END), 0) AS unknown_status_count
        FROM request_logs l
        LEFT JOIN routing_request_cost_aggregates a ON a.request_id = l.id
        WHERE l.received_at_ms >= ? AND l.received_at_ms < ?
        "#,
    )
    .bind(start_ms)
    .bind(end_ms)
    .fetch_one(&mut *connection)
    .await?;

    let single_currency_rows = sqlx::query(
        r#"
        SELECT
            upper(trim(a.compatibility_currency)) AS currency,
            SUM(a.compatibility_total_cost_micro) AS amount_micro,
            COUNT(*) AS request_count
        FROM request_logs l
        JOIN routing_request_cost_aggregates a ON a.request_id = l.id
        WHERE l.received_at_ms >= ? AND l.received_at_ms < ?
          AND a.status = 'complete_single_currency'
          AND a.compatibility_currency IS NOT NULL
          AND a.compatibility_total_cost_micro IS NOT NULL
          AND length(upper(trim(a.compatibility_currency))) BETWEEN 3 AND ?
          AND upper(trim(a.compatibility_currency)) NOT GLOB '*[^A-Z]*'
          AND a.compatibility_total_cost_micro >= 0
        GROUP BY upper(trim(a.compatibility_currency))
        ORDER BY currency ASC
        "#,
    )
    .bind(start_ms)
    .bind(end_ms)
    .bind(MAX_CURRENCY_BYTES as i64)
    .fetch_all(&mut *connection)
    .await?;

    let non_single_rows = sqlx::query(
        r#"
        WITH scoped AS (
            SELECT a.request_id, a.totals_by_currency_json
            FROM request_logs l
            JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            WHERE l.received_at_ms >= ? AND l.received_at_ms < ?
              AND a.status IN (
                'complete_mixed_currency',
                'incomplete',
                'not_applicable',
                'no_attempts'
              )
        ),
        shaped AS (
            SELECT
                request_id,
                totals_by_currency_json,
                CASE
                    WHEN json_valid(totals_by_currency_json)
                     AND json_type(totals_by_currency_json) = 'object'
                    THEN (
                        SELECT COUNT(*)
                        FROM json_each(totals_by_currency_json)
                    )
                    ELSE NULL
                END AS currency_count
            FROM scoped
        ),
        entries AS (
            SELECT
                upper(trim(json_each.key)) AS currency,
                json_each.atom AS amount_micro
            FROM shaped
            JOIN json_each(shaped.totals_by_currency_json)
            WHERE shaped.currency_count IS NOT NULL
              AND shaped.currency_count <= ?
        )
        SELECT
            'total' AS row_kind,
            currency,
            SUM(amount_micro) AS amount_micro,
            COUNT(*) AS request_count
        FROM entries
        WHERE length(currency) BETWEEN 3 AND ?
          AND currency NOT GLOB '*[^A-Z]*'
          AND typeof(amount_micro) = 'integer'
          AND amount_micro >= 0
        GROUP BY currency
        UNION ALL
        SELECT
            'corrupt_shape' AS row_kind,
            NULL AS currency,
            COUNT(*) AS amount_micro,
            0 AS request_count
        FROM shaped
        WHERE currency_count IS NULL OR currency_count > ?
        UNION ALL
        SELECT
            'corrupt_entry' AS row_kind,
            NULL AS currency,
            COUNT(*) AS amount_micro,
            0 AS request_count
        FROM entries
        WHERE NOT (
            length(currency) BETWEEN 3 AND ?
            AND currency NOT GLOB '*[^A-Z]*'
            AND typeof(amount_micro) = 'integer'
            AND amount_micro >= 0
        )
        ORDER BY row_kind ASC, currency ASC
        "#,
    )
    .bind(start_ms)
    .bind(end_ms)
    .bind(MAX_COST_CURRENCIES_PER_ROW as i64)
    .bind(MAX_CURRENCY_BYTES as i64)
    .bind(MAX_COST_CURRENCIES_PER_ROW as i64)
    .bind(MAX_CURRENCY_BYTES as i64)
    .fetch_all(&mut *connection)
    .await?;

    let invalid_single_projection_count = scalar_count_bind3(
        connection,
        r#"
        SELECT COUNT(*)
        FROM request_logs l
        JOIN routing_request_cost_aggregates a ON a.request_id = l.id
        WHERE l.received_at_ms >= ? AND l.received_at_ms < ?
          AND a.status = 'complete_single_currency'
          AND (
            a.compatibility_currency IS NULL
            OR a.compatibility_total_cost_micro IS NULL
            OR length(upper(trim(a.compatibility_currency))) NOT BETWEEN 3 AND ?
            OR upper(trim(a.compatibility_currency)) GLOB '*[^A-Z]*'
            OR a.compatibility_total_cost_micro < 0
          )
        "#,
        start_ms,
        end_ms,
        MAX_CURRENCY_BYTES as i64,
    )
    .await?;

    let mut metrics = DashboardCostMetrics {
        legacy_or_missing_aggregate_count: non_negative(
            counts.get("legacy_or_missing_aggregate_count"),
        )?,
        complete_single_currency_count: non_negative(counts.get("complete_single_currency_count"))?,
        complete_mixed_currency_count: non_negative(counts.get("complete_mixed_currency_count"))?,
        incomplete_count: non_negative(counts.get("incomplete_count"))?,
        not_applicable_count: non_negative(counts.get("not_applicable_count"))?,
        no_attempts_count: non_negative(counts.get("no_attempts_count"))?,
        ..Default::default()
    };
    for row in single_currency_rows {
        metrics.totals.push(DashboardCostTotal {
            currency: row.try_get("currency")?,
            amount_micro: row.try_get("amount_micro")?,
            request_count: non_negative(row.try_get("request_count")?)?,
        });
    }
    let mut non_single_totals = Vec::new();
    let mut corrupt_shape_count = 0u64;
    let mut corrupt_entry_count = 0u64;
    for row in non_single_rows {
        let row_kind: String = row.try_get("row_kind")?;
        match row_kind.as_str() {
            "total" => non_single_totals.push(DashboardCostTotal {
                currency: row.try_get("currency")?,
                amount_micro: row.try_get("amount_micro")?,
                request_count: non_negative(row.try_get("request_count")?)?,
            }),
            "corrupt_shape" => {
                corrupt_shape_count = non_negative(row.try_get("amount_micro")?)?;
            }
            "corrupt_entry" => {
                corrupt_entry_count = non_negative(row.try_get("amount_micro")?)?;
            }
            _ => {
                return Err(PersistenceError::InvariantViolation(
                    "unknown dashboard cost aggregate row kind".to_string(),
                ));
            }
        }
    }
    merge_cost_totals(&mut metrics.totals, non_single_totals)?;
    let corrupt = non_negative(counts.get("unknown_status_count"))?
        .checked_add(invalid_single_projection_count)
        .and_then(|value| value.checked_add(corrupt_shape_count))
        .and_then(|value| value.checked_add(corrupt_entry_count))
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "dashboard corrupt cost count overflow".to_string(),
            )
        })?;
    metrics.cost_totals_complete = metrics.incomplete_count == 0
        && metrics.legacy_or_missing_aggregate_count == 0
        && corrupt == 0;
    Ok(RawCostMetrics {
        metrics,
        corrupt_cost_aggregate_count: corrupt,
    })
}

fn bucket_floor_ms(value: i64) -> i64 {
    value.div_euclid(ROLLUP_BUCKET_MS) * ROLLUP_BUCKET_MS
}

fn bucket_ceil_ms(value: i64) -> i64 {
    if value <= 0 {
        0
    } else {
        bucket_floor_ms(value.saturating_add(ROLLUP_BUCKET_MS - 1))
    }
}

fn add_period_metrics(target: &mut RawPeriod, delta: RawPeriod) -> Result<(), PersistenceError> {
    target.period.request_count = target
        .period
        .request_count
        .checked_add(delta.period.request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.terminal_count = target
        .period
        .terminal_count
        .checked_add(delta.period.terminal_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.success_count = target
        .period
        .success_count
        .checked_add(delta.period.success_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.failed_count = target
        .period
        .failed_count
        .checked_add(delta.period.failed_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.interrupted_count = target
        .period
        .interrupted_count
        .checked_add(delta.period.interrupted_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.in_progress_count = target
        .period
        .in_progress_count
        .checked_add(delta.period.in_progress_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.prompt_tokens = target
        .period
        .prompt_tokens
        .checked_add(delta.period.prompt_tokens)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.completion_tokens = target
        .period
        .completion_tokens
        .checked_add(delta.period.completion_tokens)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.total_tokens = target
        .period
        .total_tokens
        .checked_add(delta.period.total_tokens)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.known_usage_request_count = target
        .period
        .known_usage_request_count
        .checked_add(delta.period.known_usage_request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.missing_usage_request_count = target
        .period
        .missing_usage_request_count
        .checked_add(delta.period.missing_usage_request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.stream_usage_missing_request_count = target
        .period
        .stream_usage_missing_request_count
        .checked_add(delta.period.stream_usage_missing_request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.not_applicable_usage_request_count = target
        .period
        .not_applicable_usage_request_count
        .checked_add(delta.period.not_applicable_usage_request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.unknown_usage_request_count = target
        .period
        .unknown_usage_request_count
        .checked_add(delta.period.unknown_usage_request_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.total_duration_ms = target
        .period
        .total_duration_ms
        .checked_add(delta.period.total_duration_ms)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.duration_sample_count = target
        .period
        .duration_sample_count
        .checked_add(delta.period.duration_sample_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.first_token_total_ms = target
        .period
        .first_token_total_ms
        .checked_add(delta.period.first_token_total_ms)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.period.first_token_sample_count = target
        .period
        .first_token_sample_count
        .checked_add(delta.period.first_token_sample_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.invalid_duration_count = target
        .invalid_duration_count
        .checked_add(delta.invalid_duration_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    target.unknown_lifecycle_count = target
        .unknown_lifecycle_count
        .checked_add(delta.unknown_lifecycle_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard period overflow".to_string())
        })?;
    Ok(())
}

fn subtract_period_metrics(
    target: &mut DashboardPeriodMetrics,
    delta: DashboardPeriodMetrics,
) -> Result<(), PersistenceError> {
    macro_rules! sub_field {
        ($field:ident) => {
            target.$field = target.$field.checked_sub(delta.$field).ok_or_else(|| {
                PersistenceError::InvariantViolation("dashboard period underflow".to_string())
            })?;
        };
    }
    sub_field!(request_count);
    sub_field!(terminal_count);
    sub_field!(success_count);
    sub_field!(failed_count);
    sub_field!(interrupted_count);
    sub_field!(in_progress_count);
    sub_field!(prompt_tokens);
    sub_field!(completion_tokens);
    sub_field!(total_tokens);
    sub_field!(known_usage_request_count);
    sub_field!(missing_usage_request_count);
    sub_field!(stream_usage_missing_request_count);
    sub_field!(not_applicable_usage_request_count);
    sub_field!(unknown_usage_request_count);
    sub_field!(total_duration_ms);
    sub_field!(duration_sample_count);
    sub_field!(first_token_total_ms);
    sub_field!(first_token_sample_count);
    target.finish_averages();
    Ok(())
}

fn add_cost_metrics(
    target: &mut RawCostMetrics,
    delta: RawCostMetrics,
) -> Result<(), PersistenceError> {
    add_cost_counts(&mut target.metrics, &delta.metrics)?;
    target.corrupt_cost_aggregate_count = target
        .corrupt_cost_aggregate_count
        .checked_add(delta.corrupt_cost_aggregate_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.metrics.cost_totals_complete = target.metrics.incomplete_count == 0
        && target.metrics.legacy_or_missing_aggregate_count == 0
        && target.corrupt_cost_aggregate_count == 0;
    Ok(())
}

fn subtract_cost_metrics(
    target: &mut DashboardCostMetrics,
    delta: DashboardCostMetrics,
) -> Result<(), PersistenceError> {
    target.legacy_or_missing_aggregate_count = target
        .legacy_or_missing_aggregate_count
        .checked_sub(delta.legacy_or_missing_aggregate_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    target.complete_single_currency_count = target
        .complete_single_currency_count
        .checked_sub(delta.complete_single_currency_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    target.complete_mixed_currency_count = target
        .complete_mixed_currency_count
        .checked_sub(delta.complete_mixed_currency_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    target.incomplete_count = target
        .incomplete_count
        .checked_sub(delta.incomplete_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    target.not_applicable_count = target
        .not_applicable_count
        .checked_sub(delta.not_applicable_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    target.no_attempts_count = target
        .no_attempts_count
        .checked_sub(delta.no_attempts_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
        })?;
    subtract_cost_totals(&mut target.totals, delta.totals)?;
    target.cost_totals_complete =
        target.incomplete_count == 0 && target.legacy_or_missing_aggregate_count == 0;
    Ok(())
}

fn add_cost_counts(
    target: &mut DashboardCostMetrics,
    delta: &DashboardCostMetrics,
) -> Result<(), PersistenceError> {
    target.legacy_or_missing_aggregate_count = target
        .legacy_or_missing_aggregate_count
        .checked_add(delta.legacy_or_missing_aggregate_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.complete_single_currency_count = target
        .complete_single_currency_count
        .checked_add(delta.complete_single_currency_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.complete_mixed_currency_count = target
        .complete_mixed_currency_count
        .checked_add(delta.complete_mixed_currency_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.incomplete_count = target
        .incomplete_count
        .checked_add(delta.incomplete_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.not_applicable_count = target
        .not_applicable_count
        .checked_add(delta.not_applicable_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    target.no_attempts_count = target
        .no_attempts_count
        .checked_add(delta.no_attempts_count)
        .ok_or_else(|| {
            PersistenceError::InvariantViolation("dashboard cost overflow".to_string())
        })?;
    merge_cost_totals(&mut target.totals, delta.totals.clone())?;
    target.cost_totals_complete =
        target.incomplete_count == 0 && target.legacy_or_missing_aggregate_count == 0;
    Ok(())
}

fn subtract_cost_totals(
    totals: &mut Vec<DashboardCostTotal>,
    delta_totals: Vec<DashboardCostTotal>,
) -> Result<(), PersistenceError> {
    let mut map: BTreeMap<String, (i64, u64)> = totals
        .drain(..)
        .map(|total| (total.currency, (total.amount_micro, total.request_count)))
        .collect();
    for delta in delta_totals {
        let remove_key = {
            let entry = map.get_mut(&delta.currency).ok_or_else(|| {
                PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
            })?;
            entry.0 = entry.0.checked_sub(delta.amount_micro).ok_or_else(|| {
                PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
            })?;
            entry.1 = entry.1.checked_sub(delta.request_count).ok_or_else(|| {
                PersistenceError::InvariantViolation("dashboard cost underflow".to_string())
            })?;
            entry.0 == 0 && entry.1 == 0
        };
        if remove_key {
            map.remove(&delta.currency);
        }
    }
    *totals = map
        .into_iter()
        .map(
            |(currency, (amount_micro, request_count))| DashboardCostTotal {
                currency,
                amount_micro,
                request_count,
            },
        )
        .collect();
    totals.sort_by(|left, right| left.currency.cmp(&right.currency));
    Ok(())
}

fn merge_cost_totals(
    totals: &mut Vec<DashboardCostTotal>,
    extra_totals: Vec<DashboardCostTotal>,
) -> Result<(), PersistenceError> {
    for extra in extra_totals {
        if let Some(existing) = totals
            .iter_mut()
            .find(|total| total.currency == extra.currency)
        {
            existing.amount_micro = existing
                .amount_micro
                .checked_add(extra.amount_micro)
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation(
                        "dashboard cost total overflow".to_string(),
                    )
                })?;
            existing.request_count = existing
                .request_count
                .checked_add(extra.request_count)
                .ok_or_else(|| {
                    PersistenceError::InvariantViolation(
                        "dashboard cost request count overflow".to_string(),
                    )
                })?;
        } else {
            totals.push(extra);
        }
    }
    totals.sort_by(|left, right| left.currency.cmp(&right.currency));
    Ok(())
}

fn non_negative(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| {
        PersistenceError::InvariantViolation("negative dashboard aggregate".to_string())
    })
}

async fn scalar_count(
    connection: &mut SqliteConnection,
    sql: &str,
) -> Result<u64, PersistenceError> {
    let row = sqlx::query(sql).fetch_one(&mut *connection).await?;
    non_negative(row.try_get::<i64, _>(0)?)
}

async fn scalar_count_bind(
    connection: &mut SqliteConnection,
    sql: &str,
    value: i64,
) -> Result<u64, PersistenceError> {
    let row = sqlx::query(sql)
        .bind(value)
        .fetch_one(&mut *connection)
        .await?;
    non_negative(row.try_get::<i64, _>(0)?)
}

async fn scalar_count_bind3(
    connection: &mut SqliteConnection,
    sql: &str,
    start_ms: i64,
    end_ms: i64,
    extra: i64,
) -> Result<u64, PersistenceError> {
    let row = sqlx::query(sql)
        .bind(start_ms)
        .bind(end_ms)
        .bind(extra)
        .fetch_one(&mut *connection)
        .await?;
    non_negative(row.try_get::<i64, _>(0)?)
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, Row, SqliteConnection};

    use super::*;

    async fn test_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::connect(":memory:").await.unwrap();
        connection
            .execute("PRAGMA foreign_keys = ON")
            .await
            .unwrap();
        connection
            .execute(
                r#"
                CREATE TABLE request_logs (
                    id TEXT PRIMARY KEY,
                    received_at_ms INTEGER,
                    terminal_at_ms INTEGER,
                    status TEXT NOT NULL,
                    usage_status TEXT NOT NULL,
                    prompt_tokens INTEGER,
                    completion_tokens INTEGER,
                    total_tokens INTEGER,
                    duration_ms INTEGER,
                    first_token_ms INTEGER,
                    lifecycle_status TEXT
                );
                "#,
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE INDEX idx_request_logs_received_at_ms ON request_logs(received_at_ms);",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE INDEX idx_request_logs_dashboard_metrics_range ON request_logs(
                    received_at_ms, terminal_at_ms, status, usage_status, prompt_tokens,
                    completion_tokens, total_tokens, duration_ms, first_token_ms, lifecycle_status
                );",
            )
            .await
            .unwrap();
        connection
            .execute(
                r#"
                CREATE TABLE dashboard_request_metric_rollups (
                    bucket_kind TEXT NOT NULL,
                    bucket_start_ms INTEGER NOT NULL,
                    request_count INTEGER NOT NULL DEFAULT 0,
                    terminal_count INTEGER NOT NULL DEFAULT 0,
                    success_count INTEGER NOT NULL DEFAULT 0,
                    failed_count INTEGER NOT NULL DEFAULT 0,
                    interrupted_count INTEGER NOT NULL DEFAULT 0,
                    in_progress_count INTEGER NOT NULL DEFAULT 0,
                    prompt_tokens INTEGER NOT NULL DEFAULT 0,
                    completion_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    known_usage_request_count INTEGER NOT NULL DEFAULT 0,
                    missing_usage_request_count INTEGER NOT NULL DEFAULT 0,
                    stream_usage_missing_request_count INTEGER NOT NULL DEFAULT 0,
                    not_applicable_usage_request_count INTEGER NOT NULL DEFAULT 0,
                    unknown_usage_request_count INTEGER NOT NULL DEFAULT 0,
                    total_duration_ms INTEGER NOT NULL DEFAULT 0,
                    invalid_duration_count INTEGER NOT NULL DEFAULT 0,
                    duration_sample_count INTEGER NOT NULL DEFAULT 0,
                    first_token_total_ms INTEGER NOT NULL DEFAULT 0,
                    first_token_sample_count INTEGER NOT NULL DEFAULT 0,
                    unknown_lifecycle_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (bucket_kind, bucket_start_ms)
                );

                CREATE TABLE dashboard_request_cost_rollups (
                    bucket_kind TEXT NOT NULL,
                    bucket_start_ms INTEGER NOT NULL,
                    legacy_or_missing_aggregate_count INTEGER NOT NULL DEFAULT 0,
                    complete_single_currency_count INTEGER NOT NULL DEFAULT 0,
                    complete_mixed_currency_count INTEGER NOT NULL DEFAULT 0,
                    incomplete_count INTEGER NOT NULL DEFAULT 0,
                    not_applicable_count INTEGER NOT NULL DEFAULT 0,
                    no_attempts_count INTEGER NOT NULL DEFAULT 0,
                    corrupt_cost_aggregate_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (bucket_kind, bucket_start_ms)
                );

                CREATE TABLE dashboard_request_cost_totals_rollups (
                    bucket_kind TEXT NOT NULL,
                    bucket_start_ms INTEGER NOT NULL,
                    currency TEXT NOT NULL,
                    amount_micro INTEGER NOT NULL DEFAULT 0,
                    request_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (bucket_kind, bucket_start_ms, currency)
                );
                "#,
            )
            .await
            .unwrap();
        connection
            .execute(
                r#"
                CREATE TABLE routing_request_cost_aggregates (
                    request_id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
                    status TEXT,
                    totals_by_currency_json TEXT,
                    compatibility_currency TEXT,
                    compatibility_total_cost_micro INTEGER
                );
                "#,
            )
            .await
            .unwrap();
        connection
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_log(
        connection: &mut SqliteConnection,
        id: &str,
        received_at_ms: Option<i64>,
        terminal_at_ms: Option<i64>,
        status: &str,
        usage_status: &str,
        prompt_tokens: Option<i64>,
        completion_tokens: Option<i64>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
        first_token_ms: Option<i64>,
        lifecycle_status: Option<&str>,
    ) {
        sqlx::query(
            r#"
            INSERT INTO request_logs (
                id, received_at_ms, terminal_at_ms, status, usage_status,
                prompt_tokens, completion_tokens, total_tokens, duration_ms,
                first_token_ms, lifecycle_status
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(received_at_ms)
        .bind(terminal_at_ms)
        .bind(status)
        .bind(usage_status)
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(total_tokens)
        .bind(duration_ms)
        .bind(first_token_ms)
        .bind(lifecycle_status)
        .execute(&mut *connection)
        .await
        .unwrap();
        apply_request_rollup_insert(
            connection,
            received_at_ms,
            terminal_at_ms,
            status,
            usage_status,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            first_token_ms,
            lifecycle_status,
        )
        .await;
    }

    async fn insert_cost(
        connection: &mut SqliteConnection,
        request_id: &str,
        status: Option<&str>,
        totals_by_currency_json: Option<&str>,
    ) {
        let (compatibility_currency, compatibility_total_cost_micro) =
            match (status, totals_by_currency_json) {
                (Some("complete_single_currency"), Some(json)) => {
                    match serde_json::from_str::<serde_json::Value>(json)
                        .ok()
                        .and_then(|value| {
                            let object = value.as_object()?;
                            if object.len() != 1 {
                                return None;
                            }
                            let (currency, amount) = object.iter().next()?;
                            let normalized = currency.trim().to_ascii_uppercase();
                            let valid_currency = normalized.len() <= MAX_CURRENCY_BYTES
                                && normalized.len() >= 3
                                && normalized.bytes().all(|byte| byte.is_ascii_uppercase());
                            let amount = amount.as_i64().filter(|amount| *amount >= 0)?;
                            valid_currency.then_some((normalized, amount))
                        }) {
                        Some((currency, amount)) => (Some(currency), Some(amount)),
                        None => (None, None),
                    }
                }
                _ => (None, None),
            };
        sqlx::query(
            "INSERT INTO routing_request_cost_aggregates (
                request_id, status, totals_by_currency_json,
                compatibility_currency, compatibility_total_cost_micro
             ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(request_id)
        .bind(status)
        .bind(totals_by_currency_json)
        .bind(&compatibility_currency)
        .bind(compatibility_total_cost_micro)
        .execute(&mut *connection)
        .await
        .unwrap();
        apply_cost_rollup_insert(
            connection,
            request_id,
            status,
            totals_by_currency_json,
            compatibility_currency.as_deref(),
            compatibility_total_cost_micro,
        )
        .await;
    }

    #[derive(Debug, Default, Clone)]
    struct RequestMetricDelta {
        period: DashboardPeriodMetrics,
        invalid_duration_count: u64,
        unknown_lifecycle_count: u64,
    }

    #[derive(Debug, Default, Clone)]
    struct CostRollupDelta {
        legacy_or_missing_aggregate_count: i64,
        complete_single_currency_count: i64,
        complete_mixed_currency_count: i64,
        incomplete_count: i64,
        not_applicable_count: i64,
        no_attempts_count: i64,
        corrupt_cost_aggregate_count: i64,
    }

    fn request_metric_delta(
        received_at_ms: Option<i64>,
        terminal_at_ms: Option<i64>,
        status: &str,
        usage_status: &str,
        prompt_tokens: Option<i64>,
        completion_tokens: Option<i64>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
        first_token_ms: Option<i64>,
        lifecycle_status: Option<&str>,
    ) -> RequestMetricDelta {
        let terminal = terminal_at_ms.is_some();
        let request_count = if received_at_ms.filter(|value| *value > 0).is_some() {
            1
        } else {
            0
        };
        let complete = terminal && usage_status == "complete";
        let duration_is_valid = terminal && duration_ms.unwrap_or(-1) >= 0;
        let first_token_is_valid = terminal && first_token_ms.unwrap_or(-1) >= 0;
        let mut period = DashboardPeriodMetrics {
            request_count,
            terminal_count: if terminal { 1 } else { 0 },
            success_count: if status == "success" { 1 } else { 0 },
            failed_count: if status == "failed" { 1 } else { 0 },
            interrupted_count: if status == "interrupted" { 1 } else { 0 },
            in_progress_count: if !terminal || usage_status == "in_progress" {
                1
            } else {
                0
            },
            prompt_tokens: if complete {
                prompt_tokens.unwrap_or_default().max(0) as u64
            } else {
                0
            },
            completion_tokens: if complete {
                completion_tokens.unwrap_or_default().max(0) as u64
            } else {
                0
            },
            total_tokens: if complete {
                total_tokens.unwrap_or_default().max(0) as u64
            } else {
                0
            },
            known_usage_request_count: if complete && total_tokens.is_some() {
                1
            } else {
                0
            },
            missing_usage_request_count: if terminal
                && matches!(usage_status, "missing_usage" | "stream_usage_missing")
            {
                1
            } else {
                0
            },
            stream_usage_missing_request_count: if terminal
                && usage_status == "stream_usage_missing"
            {
                1
            } else {
                0
            },
            not_applicable_usage_request_count: if terminal && usage_status == "not_applicable" {
                1
            } else {
                0
            },
            unknown_usage_request_count: if terminal && usage_status == "unknown_legacy" {
                1
            } else {
                0
            },
            total_duration_ms: if duration_is_valid {
                duration_ms.unwrap_or_default().max(0) as u64
            } else {
                0
            },
            duration_sample_count: if duration_is_valid { 1 } else { 0 },
            first_token_total_ms: if first_token_is_valid {
                first_token_ms.unwrap_or_default().max(0) as u64
            } else {
                0
            },
            first_token_sample_count: if first_token_is_valid { 1 } else { 0 },
            avg_total_duration_ms: None,
            avg_first_token_ms: None,
        };
        period.finish_averages();
        RequestMetricDelta {
            period,
            invalid_duration_count: if terminal && duration_ms.unwrap_or(-1) < 0 {
                1
            } else {
                0
            },
            unknown_lifecycle_count: match lifecycle_status {
                Some("admitted")
                | Some("completed")
                | Some("partial_success")
                | Some("failed")
                | Some("interrupted") => 0,
                _ => 1,
            },
        }
    }

    async fn upsert_request_metric_rollup(
        connection: &mut SqliteConnection,
        bucket_kind: &str,
        bucket_start_ms: i64,
        delta: &RequestMetricDelta,
    ) {
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
        .bind(i64::try_from(delta.period.request_count).unwrap())
        .bind(i64::try_from(delta.period.terminal_count).unwrap())
        .bind(i64::try_from(delta.period.success_count).unwrap())
        .bind(i64::try_from(delta.period.failed_count).unwrap())
        .bind(i64::try_from(delta.period.interrupted_count).unwrap())
        .bind(i64::try_from(delta.period.in_progress_count).unwrap())
        .bind(i64::try_from(delta.period.prompt_tokens).unwrap())
        .bind(i64::try_from(delta.period.completion_tokens).unwrap())
        .bind(i64::try_from(delta.period.total_tokens).unwrap())
        .bind(i64::try_from(delta.period.known_usage_request_count).unwrap())
        .bind(i64::try_from(delta.period.missing_usage_request_count).unwrap())
        .bind(i64::try_from(delta.period.stream_usage_missing_request_count).unwrap())
        .bind(i64::try_from(delta.period.not_applicable_usage_request_count).unwrap())
        .bind(i64::try_from(delta.period.unknown_usage_request_count).unwrap())
        .bind(i64::try_from(delta.period.total_duration_ms).unwrap())
        .bind(i64::try_from(delta.invalid_duration_count).unwrap())
        .bind(i64::try_from(delta.period.duration_sample_count).unwrap())
        .bind(i64::try_from(delta.period.first_token_total_ms).unwrap())
        .bind(i64::try_from(delta.period.first_token_sample_count).unwrap())
        .bind(i64::try_from(delta.unknown_lifecycle_count).unwrap())
        .execute(connection)
        .await
        .unwrap();
    }

    async fn upsert_cost_rollup(
        connection: &mut SqliteConnection,
        bucket_kind: &str,
        bucket_start_ms: i64,
        delta: &CostRollupDelta,
    ) {
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
        .execute(connection)
        .await
        .unwrap();
    }

    async fn upsert_cost_total_rollup(
        connection: &mut SqliteConnection,
        bucket_kind: &str,
        bucket_start_ms: i64,
        total: &DashboardCostTotal,
    ) {
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
        .bind(i64::try_from(total.request_count).unwrap())
        .execute(connection)
        .await
        .unwrap();
    }

    async fn apply_request_rollup_insert(
        connection: &mut SqliteConnection,
        received_at_ms: Option<i64>,
        terminal_at_ms: Option<i64>,
        status: &str,
        usage_status: &str,
        prompt_tokens: Option<i64>,
        completion_tokens: Option<i64>,
        total_tokens: Option<i64>,
        duration_ms: Option<i64>,
        first_token_ms: Option<i64>,
        lifecycle_status: Option<&str>,
    ) {
        let Some(received_at_ms) = received_at_ms.filter(|value| *value > 0) else {
            return;
        };
        let delta = request_metric_delta(
            Some(received_at_ms),
            terminal_at_ms,
            status,
            usage_status,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            first_token_ms,
            lifecycle_status,
        );
        for &(bucket_kind, bucket_start_ms) in [
            (ROLLUP_KIND_SECOND, bucket_floor_ms(received_at_ms)),
            (ROLLUP_KIND_LIFETIME, 0),
        ]
        .iter()
        {
            upsert_request_metric_rollup(connection, bucket_kind, bucket_start_ms, &delta).await;
        }
        let missing = CostRollupDelta {
            legacy_or_missing_aggregate_count: 1,
            ..Default::default()
        };
        for &(bucket_kind, bucket_start_ms) in [
            (ROLLUP_KIND_SECOND, bucket_floor_ms(received_at_ms)),
            (ROLLUP_KIND_LIFETIME, 0),
        ]
        .iter()
        {
            upsert_cost_rollup(connection, bucket_kind, bucket_start_ms, &missing).await;
        }
    }

    async fn apply_cost_rollup_insert(
        connection: &mut SqliteConnection,
        request_id: &str,
        status: Option<&str>,
        totals_by_currency_json: Option<&str>,
        compatibility_currency: Option<&str>,
        compatibility_total_cost_micro: Option<i64>,
    ) {
        let received_at_ms: Option<i64> =
            sqlx::query_scalar("SELECT received_at_ms FROM request_logs WHERE id = ?")
                .bind(request_id)
                .fetch_one(&mut *connection)
                .await
                .ok();
        let Some(received_at_ms) = received_at_ms.filter(|value| *value > 0) else {
            return;
        };
        let bucket_start_ms = bucket_floor_ms(received_at_ms);
        let mut delta = CostRollupDelta {
            legacy_or_missing_aggregate_count: -1,
            ..Default::default()
        };
        let mut totals = Vec::new();
        match status {
            Some("complete_single_currency") => {
                delta.complete_single_currency_count = 1;
                match (compatibility_currency, compatibility_total_cost_micro) {
                    (Some(currency), Some(amount_micro)) => {
                        let normalized = currency.trim().to_ascii_uppercase();
                        if normalized.len() >= 3
                            && normalized.len() <= MAX_CURRENCY_BYTES
                            && normalized.bytes().all(|byte| byte.is_ascii_uppercase())
                            && amount_micro >= 0
                        {
                            totals.push(DashboardCostTotal {
                                currency: normalized,
                                amount_micro,
                                request_count: 1,
                            });
                        } else {
                            delta.corrupt_cost_aggregate_count = 1;
                        }
                    }
                    _ => {
                        delta.corrupt_cost_aggregate_count = 1;
                    }
                }
            }
            Some("complete_mixed_currency")
            | Some("incomplete")
            | Some("not_applicable")
            | Some("no_attempts") => {
                match status {
                    Some("complete_mixed_currency") => delta.complete_mixed_currency_count = 1,
                    Some("incomplete") => delta.incomplete_count = 1,
                    Some("not_applicable") => delta.not_applicable_count = 1,
                    Some("no_attempts") => delta.no_attempts_count = 1,
                    _ => {}
                }
                match totals_by_currency_json {
                    Some(json) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
                            if let Some(object) = value.as_object() {
                                if object.len() <= MAX_COST_CURRENCIES_PER_ROW {
                                    for (currency, amount) in object {
                                        let normalized = currency.trim().to_ascii_uppercase();
                                        let valid_currency = normalized.len() >= 3
                                            && normalized.len() <= MAX_CURRENCY_BYTES
                                            && normalized
                                                .bytes()
                                                .all(|byte| byte.is_ascii_uppercase());
                                        let Some(amount) =
                                            amount.as_i64().filter(|value| *value >= 0)
                                        else {
                                            delta.corrupt_cost_aggregate_count += 1;
                                            continue;
                                        };
                                        if valid_currency {
                                            totals.push(DashboardCostTotal {
                                                currency: normalized,
                                                amount_micro: amount,
                                                request_count: 1,
                                            });
                                        } else {
                                            delta.corrupt_cost_aggregate_count += 1;
                                        }
                                    }
                                } else {
                                    delta.corrupt_cost_aggregate_count = 1;
                                }
                            } else {
                                delta.corrupt_cost_aggregate_count = 1;
                            }
                        } else {
                            delta.corrupt_cost_aggregate_count = 1;
                        }
                    }
                    None => delta.corrupt_cost_aggregate_count = 1,
                }
            }
            Some(_) | None => {
                delta.corrupt_cost_aggregate_count = 1;
            }
        }
        for &(bucket_kind, bucket_start_ms) in [
            (ROLLUP_KIND_SECOND, bucket_start_ms),
            (ROLLUP_KIND_LIFETIME, 0),
        ]
        .iter()
        {
            upsert_cost_rollup(&mut *connection, bucket_kind, bucket_start_ms, &delta).await;
            for total in &totals {
                upsert_cost_total_rollup(&mut *connection, bucket_kind, bucket_start_ms, total)
                    .await;
            }
        }
    }

    #[tokio::test]
    async fn period_metrics_respect_boundaries_and_usage_statuses() {
        let mut connection = test_connection().await;
        insert_log(
            &mut connection,
            "known-zero",
            Some(1_000),
            Some(1_100),
            "success",
            "complete",
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(12),
            Some("completed"),
        )
        .await;
        insert_log(
            &mut connection,
            "missing",
            Some(1_100),
            Some(1_200),
            "success",
            "missing_usage",
            None,
            None,
            None,
            Some(20),
            None,
            Some("completed"),
        )
        .await;
        insert_log(
            &mut connection,
            "stream-missing",
            Some(1_200),
            Some(1_300),
            "success",
            "stream_usage_missing",
            None,
            None,
            None,
            Some(30),
            None,
            Some("partial_success"),
        )
        .await;
        insert_log(
            &mut connection,
            "not-applicable",
            Some(1_300),
            Some(1_400),
            "success",
            "not_applicable",
            None,
            None,
            None,
            Some(40),
            None,
            Some("completed"),
        )
        .await;
        insert_log(
            &mut connection,
            "unknown-legacy",
            Some(1_400),
            Some(1_500),
            "failed",
            "unknown_legacy",
            None,
            None,
            None,
            Some(-1),
            None,
            Some("unexpected"),
        )
        .await;
        insert_log(
            &mut connection,
            "in-progress",
            Some(1_500),
            None,
            "in_progress",
            "in_progress",
            None,
            None,
            None,
            None,
            None,
            Some("admitted"),
        )
        .await;
        insert_log(
            &mut connection,
            "legacy-missing",
            Some(1_600),
            Some(1_650),
            "success",
            "complete",
            Some(1),
            Some(1),
            Some(2),
            Some(15),
            Some(5),
            Some("completed"),
        )
        .await;
        insert_log(
            &mut connection,
            "at-end",
            Some(2_000),
            Some(2_100),
            "success",
            "complete",
            Some(1),
            Some(1),
            Some(2),
            Some(10),
            None,
            Some("completed"),
        )
        .await;

        let result = load_period_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();

        assert_eq!(result.period.request_count, 7);
        assert_eq!(result.period.terminal_count, 6);
        assert_eq!(result.period.success_count, 5);
        assert_eq!(result.period.failed_count, 1);
        assert_eq!(result.period.in_progress_count, 1);
        assert_eq!(result.period.known_usage_request_count, 2);
        assert_eq!(result.period.missing_usage_request_count, 2);
        assert_eq!(result.period.stream_usage_missing_request_count, 1);
        assert_eq!(result.period.not_applicable_usage_request_count, 1);
        assert_eq!(result.period.unknown_usage_request_count, 1);
        assert_eq!(result.period.total_tokens, 2);
        assert_eq!(result.period.duration_sample_count, 5);
        assert_eq!(result.period.total_duration_ms, 105);
        assert_eq!(result.invalid_duration_count, 1);
        assert_eq!(result.unknown_lifecycle_count, 1);
    }

    #[tokio::test]
    async fn cost_metrics_are_actual_only_sorted_and_corruption_degrades() {
        let mut connection = test_connection().await;
        for id in [
            "usd",
            "mixed",
            "legacy-missing",
            "bad-json",
            "bad-currency",
            "incomplete",
        ] {
            insert_log(
                &mut connection,
                id,
                Some(1_000),
                Some(1_100),
                "success",
                "complete",
                Some(1),
                Some(1),
                Some(2),
                Some(10),
                None,
                Some("completed"),
            )
            .await;
        }
        insert_cost(
            &mut connection,
            "usd",
            Some("complete_single_currency"),
            Some(r#"{"USD":1250000}"#),
        )
        .await;
        insert_cost(
            &mut connection,
            "mixed",
            Some("complete_mixed_currency"),
            Some(r#"{"CNY":2000000,"USD":750000}"#),
        )
        .await;
        insert_cost(
            &mut connection,
            "bad-json",
            Some("complete_single_currency"),
            Some("{"),
        )
        .await;
        insert_cost(
            &mut connection,
            "bad-currency",
            Some("complete_single_currency"),
            Some(r#"{"US1":1}"#),
        )
        .await;
        insert_cost(
            &mut connection,
            "incomplete",
            Some("incomplete"),
            Some(r#"{}"#),
        )
        .await;

        let raw_costs = load_costs_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();
        let metrics = raw_costs.metrics;
        let corrupt = raw_costs.corrupt_cost_aggregate_count;

        assert_eq!(metrics.complete_single_currency_count, 3);
        assert_eq!(metrics.complete_mixed_currency_count, 1);
        assert_eq!(metrics.incomplete_count, 1);
        assert_eq!(metrics.legacy_or_missing_aggregate_count, 1);
        assert_eq!(corrupt, 2);
        assert!(!metrics.cost_totals_complete);
        assert_eq!(metrics.totals[0].currency, "CNY");
        assert_eq!(metrics.totals[0].amount_micro, 2_000_000);
        assert_eq!(metrics.totals[1].currency, "USD");
        assert_eq!(metrics.totals[1].amount_micro, 2_000_000);
    }

    #[tokio::test]
    async fn late_cost_aggregate_converges_without_changing_request_metrics() {
        let mut connection = test_connection().await;
        insert_log(
            &mut connection,
            "late-cost",
            Some(1_000),
            Some(1_200),
            "success",
            "complete",
            Some(10),
            Some(20),
            Some(30),
            Some(200),
            Some(50),
            Some("completed"),
        )
        .await;

        let before_period = load_period_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();
        let before_costs = load_costs_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();

        assert_eq!(before_period.period.request_count, 1);
        assert_eq!(before_costs.metrics.legacy_or_missing_aggregate_count, 1);
        assert_eq!(before_costs.corrupt_cost_aggregate_count, 0);
        assert!(!before_costs.metrics.cost_totals_complete);

        insert_cost(
            &mut connection,
            "late-cost",
            Some("complete_single_currency"),
            Some(r#"{"USD":9000}"#),
        )
        .await;

        let after_period = load_period_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();
        let after_costs = load_costs_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();

        assert_eq!(after_period.period, before_period.period);
        assert_eq!(after_costs.metrics.legacy_or_missing_aggregate_count, 0);
        assert_eq!(after_costs.corrupt_cost_aggregate_count, 0);
        assert!(after_costs.metrics.cost_totals_complete);
        assert_eq!(after_costs.metrics.totals[0].currency, "USD");
        assert_eq!(after_costs.metrics.totals[0].amount_micro, 9_000);
    }

    #[tokio::test]
    async fn interrupted_and_failed_requests_contribute_terminal_duration() {
        let mut connection = test_connection().await;
        insert_log(
            &mut connection,
            "failed",
            Some(1_000),
            Some(1_050),
            "failed",
            "missing_usage",
            None,
            None,
            None,
            Some(50),
            None,
            Some("failed"),
        )
        .await;
        insert_log(
            &mut connection,
            "interrupted",
            Some(1_100),
            Some(1_175),
            "interrupted",
            "missing_usage",
            None,
            None,
            None,
            Some(75),
            None,
            Some("interrupted"),
        )
        .await;

        let result = load_period_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();

        assert_eq!(result.period.request_count, 2);
        assert_eq!(result.period.terminal_count, 2);
        assert_eq!(result.period.failed_count, 1);
        assert_eq!(result.period.interrupted_count, 1);
        assert_eq!(result.period.duration_sample_count, 2);
        assert_eq!(result.period.avg_total_duration_ms, Some(62.5));
    }

    #[tokio::test]
    async fn clear_request_logs_zeroes_metrics_and_cascades_costs() {
        let mut connection = test_connection().await;
        insert_log(
            &mut connection,
            "clear-me",
            Some(1_000),
            Some(1_050),
            "success",
            "complete",
            Some(1),
            Some(2),
            Some(3),
            Some(50),
            None,
            Some("completed"),
        )
        .await;
        insert_cost(
            &mut connection,
            "clear-me",
            Some("complete_single_currency"),
            Some(r#"{"USD":1000}"#),
        )
        .await;

        sqlx::query("DELETE FROM request_logs")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("DELETE FROM dashboard_request_metric_rollups")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("DELETE FROM dashboard_request_cost_rollups")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("DELETE FROM dashboard_request_cost_totals_rollups")
            .execute(&mut connection)
            .await
            .unwrap();

        let remaining_cost_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_request_cost_aggregates")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        let period = load_period_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();
        let costs = load_costs_window(&mut connection, 1_000, 2_000)
            .await
            .unwrap();

        assert_eq!(remaining_cost_rows, 0);
        assert_eq!(period.period.request_count, 0);
        assert!(costs.metrics.cost_totals_complete);
        assert!(costs.metrics.totals.is_empty());
        assert_eq!(costs.corrupt_cost_aggregate_count, 0);
    }

    #[tokio::test]
    async fn canonical_timestamp_index_is_used_for_range_reads() {
        let mut connection = test_connection().await;
        let rows = sqlx::query(
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM request_logs WHERE received_at_ms >= ? AND received_at_ms < ?",
        )
        .bind(1_000_i64)
        .bind(2_000_i64)
        .fetch_all(&mut connection)
        .await
        .unwrap();
        let plan = rows
            .iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            plan.contains("idx_request_logs_received_at_ms")
                || plan.contains("idx_request_logs_dashboard_metrics_range"),
            "{plan}"
        );
    }
}
