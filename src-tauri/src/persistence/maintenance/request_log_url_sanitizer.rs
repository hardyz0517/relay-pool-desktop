use sqlx::{Row, SqliteConnection, SqlitePool};
use url::Url;

use crate::persistence::error::PersistenceError;

pub(crate) const REQUEST_LOG_URL_SANITIZER_ID: &str = "request_logs_upstream_base_url_v1";
const DEFAULT_BATCH_SIZE: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequestLogUrlSanitizerOptions {
    pub(crate) batch_size: i64,
    pub(crate) max_batches: Option<u32>,
}

impl Default for RequestLogUrlSanitizerOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            max_batches: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestLogUrlSanitizerReport {
    pub(crate) complete: bool,
    pub(crate) sanitized_count: u64,
    pub(crate) redacted_unparseable_count: u64,
    pub(crate) redacted_non_http_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyUrlSanitization {
    SanitizedOrigin { origin: String },
    RedactedUnparseable,
    RedactedNonHttp,
}

impl LegacyUrlSanitization {
    fn reason(&self) -> &'static str {
        match self {
            Self::SanitizedOrigin { .. } => "sanitized_origin",
            Self::RedactedUnparseable => "redacted_unparseable",
            Self::RedactedNonHttp => "redacted_non_http",
        }
    }
}

#[allow(
    dead_code,
    reason = "string wrapper documents the shared sanitizer primitive while production migration reads raw SQLite bytes to handle invalid UTF-8"
)]
pub(crate) fn sanitize_legacy_upstream_url(input: &str) -> LegacyUrlSanitization {
    sanitize_legacy_upstream_url_bytes(input.as_bytes())
}

pub(crate) fn sanitize_legacy_upstream_url_bytes(input: &[u8]) -> LegacyUrlSanitization {
    let Ok(input) = std::str::from_utf8(input) else {
        return LegacyUrlSanitization::RedactedUnparseable;
    };
    let Ok(mut url) = Url::parse(input.trim()) else {
        return LegacyUrlSanitization::RedactedUnparseable;
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return LegacyUrlSanitization::RedactedNonHttp;
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.set_path("");
    LegacyUrlSanitization::SanitizedOrigin {
        origin: url.to_string().trim_end_matches('/').to_string(),
    }
}

pub(crate) async fn sanitize_request_log_upstream_urls(
    pool: &SqlitePool,
    options: RequestLogUrlSanitizerOptions,
) -> Result<RequestLogUrlSanitizerReport, PersistenceError> {
    ensure_progress_row(pool).await?;
    let report = sanitize_request_log_upstream_urls_inner(pool, options, true).await?;
    if report.complete {
        compact_sanitized_request_log_storage(pool).await?;
    }
    Ok(report)
}

pub(crate) async fn sanitize_request_log_upstream_urls_before_schema18(
    pool: &SqlitePool,
    options: RequestLogUrlSanitizerOptions,
) -> Result<RequestLogUrlSanitizerReport, PersistenceError> {
    let report = sanitize_request_log_upstream_urls_inner(pool, options, false).await?;
    if report.complete {
        compact_sanitized_request_log_storage(pool).await?;
    }
    Ok(report)
}

async fn sanitize_request_log_upstream_urls_inner(
    pool: &SqlitePool,
    options: RequestLogUrlSanitizerOptions,
    record_progress: bool,
) -> Result<RequestLogUrlSanitizerReport, PersistenceError> {
    let batch_size = options.batch_size.clamp(1, 10_000);
    let mut report = RequestLogUrlSanitizerReport {
        complete: false,
        sanitized_count: 0,
        redacted_unparseable_count: 0,
        redacted_non_http_count: 0,
    };
    let mut batches = 0_u32;

    loop {
        if options
            .max_batches
            .is_some_and(|max_batches| batches >= max_batches)
        {
            return Ok(report);
        }
        batches += 1;
        let rows = sqlx::query(
            r#"
            SELECT id, CAST(upstream_base_url AS BLOB) AS upstream_base_url_bytes
            FROM request_logs
            WHERE upstream_base_url IS NOT NULL
            ORDER BY created_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(batch_size)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            if record_progress {
                mark_complete(pool).await?;
            }
            report.complete = true;
            return Ok(report);
        }

        let mut transaction = pool.begin().await?;
        if record_progress {
            sqlx::query(
                r#"
                UPDATE request_log_url_sanitizer_progress
                SET status = 'running', updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(now_millis_string())
            .bind(REQUEST_LOG_URL_SANITIZER_ID)
            .execute(&mut *transaction)
            .await?;
        }

        for row in rows {
            let id: String = row.get("id");
            let value: Vec<u8> = row.get("upstream_base_url_bytes");
            let outcome = sanitize_legacy_upstream_url_bytes(&value);
            sqlx::query(
                r#"
                UPDATE request_logs
                SET upstream_base_url = NULL
                WHERE id = ? AND upstream_base_url IS NOT NULL
                "#,
            )
            .bind(&id)
            .execute(&mut *transaction)
            .await?;
            report.sanitized_count += 1;
            match outcome {
                LegacyUrlSanitization::RedactedUnparseable => {
                    report.redacted_unparseable_count += 1;
                }
                LegacyUrlSanitization::RedactedNonHttp => {
                    report.redacted_non_http_count += 1;
                }
                LegacyUrlSanitization::SanitizedOrigin { .. } => {}
            }
            if record_progress {
                sqlx::query(
                    r#"
                    UPDATE request_log_url_sanitizer_progress
                    SET sanitized_count = sanitized_count + 1,
                        redacted_unparseable_count = redacted_unparseable_count + ?,
                        redacted_non_http_count = redacted_non_http_count + ?,
                        last_request_log_id = ?,
                        last_reason = ?,
                        updated_at = ?
                    WHERE id = ?
                    "#,
                )
                .bind(i64::from(matches!(
                    outcome,
                    LegacyUrlSanitization::RedactedUnparseable
                )))
                .bind(i64::from(matches!(
                    outcome,
                    LegacyUrlSanitization::RedactedNonHttp
                )))
                .bind(&id)
                .bind(outcome.reason())
                .bind(now_millis_string())
                .bind(REQUEST_LOG_URL_SANITIZER_ID)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
    }
}

pub(crate) async fn assert_request_log_url_sanitizer_complete_on_connection(
    connection: &mut SqliteConnection,
) -> Result<(), PersistenceError> {
    let remaining: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM request_logs
        WHERE upstream_base_url IS NOT NULL
        "#,
    )
    .fetch_one(&mut *connection)
    .await?;
    let status = sqlx::query_scalar::<_, String>(
        r#"
        SELECT status
        FROM request_log_url_sanitizer_progress
        WHERE id = ?
        "#,
    )
    .bind(REQUEST_LOG_URL_SANITIZER_ID)
    .fetch_optional(&mut *connection)
    .await?;
    if remaining == 0 && status.as_deref() == Some("complete") {
        return Ok(());
    }
    Err(PersistenceError::InvariantViolation(
        "request log upstream URL sanitizer is incomplete".to_string(),
    ))
}

async fn ensure_progress_row(pool: &SqlitePool) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO request_log_url_sanitizer_progress (
            id, status, sanitized_count, redacted_unparseable_count, redacted_non_http_count, updated_at
        ) VALUES (?, 'pending', 0, 0, 0, ?)
        "#,
    )
    .bind(REQUEST_LOG_URL_SANITIZER_ID)
    .bind(now_millis_string())
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_complete(pool: &SqlitePool) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        UPDATE request_log_url_sanitizer_progress
        SET status = 'complete',
            last_reason = 'complete',
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now_millis_string())
    .bind(REQUEST_LOG_URL_SANITIZER_ID)
    .execute(pool)
    .await?;
    Ok(())
}

async fn compact_sanitized_request_log_storage(pool: &SqlitePool) -> Result<(), PersistenceError> {
    let mut connection = pool.acquire().await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("VACUUM").execute(&mut *connection).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *connection)
        .await?;
    Ok(())
}

fn now_millis_string() -> String {
    crate::services::time::now_millis_for_services().to_string()
}
