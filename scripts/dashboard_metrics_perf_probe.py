#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
import sqlite3
import statistics
import tempfile
import threading
import time
from pathlib import Path
from typing import Any


RECENT_WINDOW_MS = 5 * 60 * 1000
MAX_COST_CURRENCIES_PER_ROW = 32
MAX_CURRENCY_BYTES = 16

PERIOD_SQL = """
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
"""

COST_COUNTS_SQL = """
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
"""

SINGLE_CURRENCY_TOTALS_SQL = """
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
"""

COST_ROWS_SQL = """
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
"""

INVALID_SINGLE_PROJECTION_SQL = """
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
"""

ROLLUP_PERIOD_SQL = """
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
"""

ROLLUP_COST_COUNTS_SQL = """
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
"""

ROLLUP_COST_TOTALS_SQL = """
SELECT
    currency,
    SUM(amount_micro) AS amount_micro,
    SUM(request_count) AS request_count
FROM dashboard_request_cost_totals_rollups
WHERE bucket_kind = ? AND bucket_start_ms >= ? AND bucket_start_ms < ?
GROUP BY currency
ORDER BY currency ASC
"""


def connect(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(path, timeout=5.0, isolation_level=None)
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA synchronous=NORMAL")
    connection.execute("PRAGMA foreign_keys=ON")
    connection.execute("PRAGMA busy_timeout=5000")
    return connection


def create_schema(connection: sqlite3.Connection) -> None:
    connection.executescript(
        """
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
        CREATE INDEX idx_request_logs_received_at
            ON request_logs(received_at_ms DESC, id DESC);
        CREATE INDEX idx_request_logs_dashboard_metrics_range
            ON request_logs(
                received_at_ms,
                terminal_at_ms,
                status,
                usage_status,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                duration_ms,
                first_token_ms,
                lifecycle_status
            );
        CREATE TABLE routing_request_cost_aggregates (
            request_id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
            status TEXT NOT NULL,
            totals_by_currency_json TEXT NOT NULL,
            compatibility_currency TEXT,
            compatibility_total_cost_micro INTEGER
        );
        CREATE TABLE dashboard_request_metric_rollups (
            bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
            bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
            request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
            terminal_count INTEGER NOT NULL DEFAULT 0 CHECK (terminal_count >= 0),
            success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
            failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
            interrupted_count INTEGER NOT NULL DEFAULT 0 CHECK (interrupted_count >= 0),
            in_progress_count INTEGER NOT NULL DEFAULT 0 CHECK (in_progress_count >= 0),
            prompt_tokens INTEGER NOT NULL DEFAULT 0 CHECK (prompt_tokens >= 0),
            completion_tokens INTEGER NOT NULL DEFAULT 0 CHECK (completion_tokens >= 0),
            total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
            known_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (known_usage_request_count >= 0),
            missing_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (missing_usage_request_count >= 0),
            stream_usage_missing_request_count INTEGER NOT NULL DEFAULT 0 CHECK (stream_usage_missing_request_count >= 0),
            not_applicable_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (not_applicable_usage_request_count >= 0),
            unknown_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (unknown_usage_request_count >= 0),
            total_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (total_duration_ms >= 0),
            invalid_duration_count INTEGER NOT NULL DEFAULT 0 CHECK (invalid_duration_count >= 0),
            duration_sample_count INTEGER NOT NULL DEFAULT 0 CHECK (duration_sample_count >= 0),
            first_token_total_ms INTEGER NOT NULL DEFAULT 0 CHECK (first_token_total_ms >= 0),
            first_token_sample_count INTEGER NOT NULL DEFAULT 0 CHECK (first_token_sample_count >= 0),
            unknown_lifecycle_count INTEGER NOT NULL DEFAULT 0 CHECK (unknown_lifecycle_count >= 0),
            PRIMARY KEY (bucket_kind, bucket_start_ms)
        );
        CREATE INDEX idx_dashboard_request_metric_rollups_range
            ON dashboard_request_metric_rollups(bucket_kind, bucket_start_ms);
        CREATE TABLE dashboard_request_cost_rollups (
            bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
            bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
            legacy_or_missing_aggregate_count INTEGER NOT NULL DEFAULT 0 CHECK (legacy_or_missing_aggregate_count >= 0),
            complete_single_currency_count INTEGER NOT NULL DEFAULT 0 CHECK (complete_single_currency_count >= 0),
            complete_mixed_currency_count INTEGER NOT NULL DEFAULT 0 CHECK (complete_mixed_currency_count >= 0),
            incomplete_count INTEGER NOT NULL DEFAULT 0 CHECK (incomplete_count >= 0),
            not_applicable_count INTEGER NOT NULL DEFAULT 0 CHECK (not_applicable_count >= 0),
            no_attempts_count INTEGER NOT NULL DEFAULT 0 CHECK (no_attempts_count >= 0),
            corrupt_cost_aggregate_count INTEGER NOT NULL DEFAULT 0 CHECK (corrupt_cost_aggregate_count >= 0),
            PRIMARY KEY (bucket_kind, bucket_start_ms)
        );
        CREATE INDEX idx_dashboard_request_cost_rollups_range
            ON dashboard_request_cost_rollups(bucket_kind, bucket_start_ms);
        CREATE TABLE dashboard_request_cost_totals_rollups (
            bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
            bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
            currency TEXT NOT NULL,
            amount_micro INTEGER NOT NULL DEFAULT 0 CHECK (amount_micro >= 0),
            request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
            PRIMARY KEY (bucket_kind, bucket_start_ms, currency)
        );
        CREATE INDEX idx_dashboard_request_cost_totals_rollups_range
            ON dashboard_request_cost_totals_rollups(bucket_kind, bucket_start_ms, currency);
        """
    )


def seed_rows(connection: sqlite3.Connection, rows: int) -> int:
    base_ms = 1_700_000_000_000
    batch_size = 10_000
    log_batch = []
    cost_batch = []
    started = time.perf_counter()
    connection.execute("BEGIN")
    for index in range(rows):
        request_id = f"req-{index:08d}"
        received_at_ms = base_ms + index
        terminal_at_ms = None if index % 97 == 0 else received_at_ms + 120 + (index % 31)
        status = "in_progress" if terminal_at_ms is None else ("failed" if index % 29 == 0 else ("interrupted" if index % 53 == 0 else "success"))
        if terminal_at_ms is None:
            usage_status = "in_progress"
            prompt_tokens = completion_tokens = total_tokens = duration_ms = first_token_ms = None
            lifecycle_status = "admitted"
        elif index % 41 == 0:
            usage_status = "stream_usage_missing"
            prompt_tokens = completion_tokens = total_tokens = None
            duration_ms = terminal_at_ms - received_at_ms
            first_token_ms = None
            lifecycle_status = "partial_success"
        elif index % 37 == 0:
            usage_status = "not_applicable"
            prompt_tokens = completion_tokens = total_tokens = None
            duration_ms = terminal_at_ms - received_at_ms
            first_token_ms = None
            lifecycle_status = "completed"
        else:
            usage_status = "complete"
            prompt_tokens = index % 400
            completion_tokens = index % 250
            total_tokens = prompt_tokens + completion_tokens
            duration_ms = terminal_at_ms - received_at_ms
            first_token_ms = 30 + (index % 20)
            lifecycle_status = "completed" if status == "success" else status
        log_batch.append(
            (
                request_id,
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
        )
        if index % 101 == 0:
            cost_status = "incomplete"
            totals = "{}"
            compatibility_currency = None
            compatibility_total_cost_micro = None
        elif index % 89 == 0:
            cost_status = "complete_mixed_currency"
            totals = '{"CNY":2000,"USD":1000}'
            compatibility_currency = None
            compatibility_total_cost_micro = None
        else:
            cost_status = "complete_single_currency"
            totals = '{"USD":1000}'
            compatibility_currency = "USD"
            compatibility_total_cost_micro = 1000
        cost_batch.append(
            (
                request_id,
                cost_status,
                totals,
                compatibility_currency,
                compatibility_total_cost_micro,
            )
        )
        if len(log_batch) >= batch_size:
            connection.executemany("INSERT INTO request_logs VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", log_batch)
            connection.executemany("INSERT INTO routing_request_cost_aggregates VALUES (?, ?, ?, ?, ?)", cost_batch)
            log_batch.clear()
            cost_batch.clear()
    if log_batch:
        connection.executemany("INSERT INTO request_logs VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", log_batch)
        connection.executemany("INSERT INTO routing_request_cost_aggregates VALUES (?, ?, ?, ?, ?)", cost_batch)
    connection.execute("COMMIT")
    rebuild_rollups(connection)
    connection.execute("ANALYZE")
    elapsed_ms = round((time.perf_counter() - started) * 1000, 3)
    return elapsed_ms


def metric_rollup_select(bucket_kind: str, bucket_expr: str) -> str:
    return f"""
        SELECT
            '{bucket_kind}' AS bucket_kind,
            {bucket_expr} AS bucket_start_ms,
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
        WHERE received_at_ms > 0
        GROUP BY {bucket_expr}
    """


def cost_rollup_select(bucket_kind: str, bucket_expr: str) -> str:
    return f"""
        SELECT
            '{bucket_kind}' AS bucket_kind,
            {bucket_expr} AS bucket_start_ms,
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
            END), 0) AS corrupt_cost_aggregate_count
        FROM request_logs l
        LEFT JOIN routing_request_cost_aggregates a ON a.request_id = l.id
        WHERE l.received_at_ms > 0
        GROUP BY {bucket_expr}
    """


def rebuild_rollups(connection: sqlite3.Connection) -> None:
    second_bucket = "(received_at_ms / 1000) * 1000"
    connection.execute("DELETE FROM dashboard_request_metric_rollups")
    connection.execute("DELETE FROM dashboard_request_cost_rollups")
    connection.execute("DELETE FROM dashboard_request_cost_totals_rollups")
    connection.execute(
        "INSERT INTO dashboard_request_metric_rollups " + metric_rollup_select("second", second_bucket)
    )
    connection.execute(
        "INSERT INTO dashboard_request_metric_rollups " + metric_rollup_select("lifetime", "CAST(0 AS INTEGER)")
    )
    connection.execute(
        "INSERT INTO dashboard_request_cost_rollups " + cost_rollup_select("second", second_bucket)
    )
    connection.execute(
        "INSERT INTO dashboard_request_cost_rollups " + cost_rollup_select("lifetime", "CAST(0 AS INTEGER)")
    )
    connection.execute(
        f"""
        INSERT INTO dashboard_request_cost_totals_rollups
        SELECT
            bucket_kind,
            bucket_start_ms,
            currency,
            SUM(amount_micro) AS amount_micro,
            SUM(request_count) AS request_count
        FROM (
            SELECT
                'second' AS bucket_kind,
                {second_bucket} AS bucket_start_ms,
                upper(trim(a.compatibility_currency)) AS currency,
                SUM(a.compatibility_total_cost_micro) AS amount_micro,
                COUNT(*) AS request_count
            FROM request_logs l
            JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            WHERE l.received_at_ms > 0
              AND a.status = 'complete_single_currency'
              AND a.compatibility_currency IS NOT NULL
              AND a.compatibility_total_cost_micro IS NOT NULL
              AND a.compatibility_total_cost_micro >= 0
            GROUP BY bucket_kind, bucket_start_ms, currency
            UNION ALL
            SELECT
                'lifetime' AS bucket_kind,
                0 AS bucket_start_ms,
                upper(trim(a.compatibility_currency)) AS currency,
                SUM(a.compatibility_total_cost_micro) AS amount_micro,
                COUNT(*) AS request_count
            FROM request_logs l
            JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            WHERE l.received_at_ms > 0
              AND a.status = 'complete_single_currency'
              AND a.compatibility_currency IS NOT NULL
              AND a.compatibility_total_cost_micro IS NOT NULL
              AND a.compatibility_total_cost_micro >= 0
            GROUP BY bucket_kind, bucket_start_ms, currency
            UNION ALL
            SELECT
                'second' AS bucket_kind,
                {second_bucket} AS bucket_start_ms,
                upper(trim(json_each.key)) AS currency,
                SUM(json_each.atom) AS amount_micro,
                COUNT(*) AS request_count
            FROM request_logs l
            JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            JOIN json_each(a.totals_by_currency_json)
            WHERE l.received_at_ms > 0
              AND a.status IN ('complete_mixed_currency', 'incomplete', 'not_applicable', 'no_attempts')
              AND json_valid(a.totals_by_currency_json)
              AND typeof(json_each.atom) = 'integer'
              AND json_each.atom >= 0
            GROUP BY bucket_kind, bucket_start_ms, currency
            UNION ALL
            SELECT
                'lifetime' AS bucket_kind,
                0 AS bucket_start_ms,
                upper(trim(json_each.key)) AS currency,
                SUM(json_each.atom) AS amount_micro,
                COUNT(*) AS request_count
            FROM request_logs l
            JOIN routing_request_cost_aggregates a ON a.request_id = l.id
            JOIN json_each(a.totals_by_currency_json)
            WHERE l.received_at_ms > 0
              AND a.status IN ('complete_mixed_currency', 'incomplete', 'not_applicable', 'no_attempts')
              AND json_valid(a.totals_by_currency_json)
              AND typeof(json_each.atom) = 'integer'
              AND json_each.atom >= 0
            GROUP BY bucket_kind, bucket_start_ms, currency
        )
        WHERE length(currency) BETWEEN 3 AND 16
          AND currency NOT GLOB '*[^A-Z]*'
        GROUP BY bucket_kind, bucket_start_ms, currency
        """
    )


def load_period(connection: sqlite3.Connection, start_ms: int, end_ms: int) -> None:
    connection.execute(ROLLUP_PERIOD_SQL, ("second", start_ms, end_ms)).fetchone()


def load_costs(connection: sqlite3.Connection, start_ms: int, end_ms: int) -> None:
    connection.execute(ROLLUP_COST_COUNTS_SQL, ("second", start_ms, end_ms)).fetchone()
    connection.execute(ROLLUP_COST_TOTALS_SQL, ("second", start_ms, end_ms)).fetchall()


def load_live(connection: sqlite3.Connection, day_start_ms: int, captured_at_ms: int) -> None:
    load_period(connection, captured_at_ms - RECENT_WINDOW_MS, captured_at_ms)
    load_period(connection, day_start_ms, captured_at_ms)
    load_costs(connection, day_start_ms, captured_at_ms)


def load_cumulative(connection: sqlite3.Connection, captured_at_ms: int) -> None:
    connection.execute(ROLLUP_PERIOD_SQL, ("lifetime", 0, 1000)).fetchone()
    connection.execute(ROLLUP_COST_COUNTS_SQL, ("lifetime", 0, 1000)).fetchone()
    connection.execute(ROLLUP_COST_TOTALS_SQL, ("lifetime", 0, 1000)).fetchall()
    connection.execute("SELECT COUNT(*) FROM request_logs WHERE received_at_ms IS NULL OR received_at_ms <= 0").fetchone()
    connection.execute("SELECT COUNT(*) FROM request_logs WHERE received_at_ms >= ?", (captured_at_ms,)).fetchone()


def sample_ms(callback, samples: int) -> list[float]:
    values = []
    for _ in range(samples):
        started = time.perf_counter()
        callback()
        values.append((time.perf_counter() - started) * 1000)
    return values


def p95(values: list[float]) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(len(ordered) * 0.95 + 0.999999) - 1))
    return ordered[index]


def stats(values: list[float]) -> dict[str, Any]:
    return {
        "samples": len(values),
        "p50Ms": round(statistics.median(values), 3),
        "p95Ms": round(p95(values), 3),
        "maxMs": round(max(values), 3),
    }


def explain(connection: sqlite3.Connection) -> dict[str, list[str]]:
    return {
        "rollupPeriodRange": [
            row[3]
            for row in connection.execute(
                "EXPLAIN QUERY PLAN " + ROLLUP_PERIOD_SQL,
                ("second", 1_700_000_000_000, 1_700_000_100_000),
            ).fetchall()
        ],
        "rollupCostCounts": [
            row[3]
            for row in connection.execute(
                "EXPLAIN QUERY PLAN " + ROLLUP_COST_COUNTS_SQL,
                ("second", 1_700_000_000_000, 1_700_000_100_000),
            ).fetchall()
        ],
        "rollupCostTotals": [
            row[3]
            for row in connection.execute(
                "EXPLAIN QUERY PLAN " + ROLLUP_COST_TOTALS_SQL,
                ("second", 1_700_000_000_000, 1_700_000_100_000),
            ).fetchall()
        ],
    }


def increment_writer_rollups(connection: sqlite3.Connection, received_at_ms: int) -> None:
    bucket_start_ms = (received_at_ms // 1000) * 1000
    for bucket_kind, bucket_start in (("second", bucket_start_ms), ("lifetime", 0)):
        connection.execute(
            """
            INSERT INTO dashboard_request_metric_rollups (
                bucket_kind,
                bucket_start_ms,
                request_count,
                terminal_count,
                success_count,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                known_usage_request_count,
                total_duration_ms,
                duration_sample_count,
                first_token_total_ms,
                first_token_sample_count
            )
            VALUES (?, ?, 1, 1, 1, 1, 2, 3, 1, 100, 1, 40, 1)
            ON CONFLICT(bucket_kind, bucket_start_ms) DO UPDATE SET
                request_count = request_count + excluded.request_count,
                terminal_count = terminal_count + excluded.terminal_count,
                success_count = success_count + excluded.success_count,
                prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                completion_tokens = completion_tokens + excluded.completion_tokens,
                total_tokens = total_tokens + excluded.total_tokens,
                known_usage_request_count = known_usage_request_count + excluded.known_usage_request_count,
                total_duration_ms = total_duration_ms + excluded.total_duration_ms,
                duration_sample_count = duration_sample_count + excluded.duration_sample_count,
                first_token_total_ms = first_token_total_ms + excluded.first_token_total_ms,
                first_token_sample_count = first_token_sample_count + excluded.first_token_sample_count
            """,
            (bucket_kind, bucket_start),
        )
        connection.execute(
            """
            INSERT INTO dashboard_request_cost_rollups (
                bucket_kind,
                bucket_start_ms,
                complete_single_currency_count
            )
            VALUES (?, ?, 1)
            ON CONFLICT(bucket_kind, bucket_start_ms) DO UPDATE SET
                complete_single_currency_count =
                    complete_single_currency_count + excluded.complete_single_currency_count
            """,
            (bucket_kind, bucket_start),
        )
        connection.execute(
            """
            INSERT INTO dashboard_request_cost_totals_rollups (
                bucket_kind,
                bucket_start_ms,
                currency,
                amount_micro,
                request_count
            )
            VALUES (?, ?, 'USD', 1000, 1)
            ON CONFLICT(bucket_kind, bucket_start_ms, currency) DO UPDATE SET
                amount_micro = amount_micro + excluded.amount_micro,
                request_count = request_count + excluded.request_count
            """,
            (bucket_kind, bucket_start),
        )


def writer_sample(connection: sqlite3.Connection, start_index: int, samples: int) -> tuple[list[float], int]:
    values = []
    busy = 0
    for offset in range(samples):
        index = start_index + offset
        request_id = f"writer-{index:08d}"
        received_at_ms = 1_700_100_000_000 + index
        started = time.perf_counter()
        try:
            connection.execute("BEGIN IMMEDIATE")
            connection.execute(
                "INSERT INTO request_logs VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    request_id,
                    received_at_ms,
                    received_at_ms + 100,
                    "success",
                    "complete",
                    1,
                    2,
                    3,
                    100,
                    40,
                    "completed",
                ),
            )
            connection.execute(
                "INSERT INTO routing_request_cost_aggregates VALUES (?, ?, ?, ?, ?)",
                (request_id, "complete_single_currency", '{"USD":1000}', "USD", 1000),
            )
            increment_writer_rollups(connection, received_at_ms)
            connection.execute("COMMIT")
        except sqlite3.OperationalError as error:
            connection.execute("ROLLBACK")
            if "busy" in str(error).lower() or "locked" in str(error).lower():
                busy += 1
            else:
                raise
        values.append((time.perf_counter() - started) * 1000)
    return values, busy


def concurrency_probe(path: Path, rows: int, day_start_ms: int, captured_at_ms: int, samples: int) -> dict[str, Any]:
    writer_connection = connect(path)
    baseline, baseline_busy = writer_sample(writer_connection, rows + 1, samples)
    writer_connection.close()

    stop = threading.Event()
    read_busy = 0
    read_lock = threading.Lock()

    def reader(kind: str) -> None:
        nonlocal read_busy
        connection = connect(path)
        while not stop.is_set():
            try:
                if kind == "live":
                    load_live(connection, day_start_ms, captured_at_ms)
                else:
                    load_cumulative(connection, captured_at_ms)
            except sqlite3.OperationalError as error:
                if "busy" in str(error).lower() or "locked" in str(error).lower():
                    with read_lock:
                        read_busy += 1
                else:
                    raise
        connection.close()

    live_thread = threading.Thread(target=reader, args=("live",), daemon=True)
    cumulative_thread = threading.Thread(target=reader, args=("cumulative",), daemon=True)
    live_thread.start()
    cumulative_thread.start()
    writer_connection = connect(path)
    concurrent, concurrent_busy = writer_sample(writer_connection, rows + samples + 1, samples)
    writer_connection.close()
    stop.set()
    live_thread.join(timeout=5)
    cumulative_thread.join(timeout=5)

    baseline_p95 = p95(baseline)
    concurrent_p95 = p95(concurrent)
    regression = 0.0 if baseline_p95 == 0 else ((concurrent_p95 - baseline_p95) / baseline_p95) * 100
    return {
        "writerBaseline": stats(baseline),
        "writerConcurrent": stats(concurrent),
        "writerP95RegressionPercent": round(regression, 3),
        "writerBusyCount": baseline_busy + concurrent_busy,
        "readerBusyCount": read_busy,
    }


def run_case(base_dir: Path, rows: int, warm_samples: int, cold_samples: int, writer_samples: int) -> dict[str, Any]:
    db_path = base_dir / f"dashboard-metrics-{rows}.sqlite3"
    connection = connect(db_path)
    create_schema(connection)
    seed_ms = seed_rows(connection, rows)
    day_start_ms = 1_700_000_000_000
    captured_at_ms = day_start_ms + rows

    live_warm = sample_ms(lambda: load_live(connection, day_start_ms, captured_at_ms), warm_samples)
    cumulative_warm = sample_ms(lambda: load_cumulative(connection, captured_at_ms), warm_samples)
    query_plan = explain(connection)
    connection.close()

    live_cold = []
    cumulative_cold = []
    for _ in range(cold_samples):
        cold_connection = connect(db_path)
        live_cold.extend(sample_ms(lambda: load_live(cold_connection, day_start_ms, captured_at_ms), 1))
        cold_connection.close()
        cold_connection = connect(db_path)
        cumulative_cold.extend(sample_ms(lambda: load_cumulative(cold_connection, captured_at_ms), 1))
        cold_connection.close()

    concurrency = concurrency_probe(db_path, rows, day_start_ms, captured_at_ms, writer_samples)
    return {
        "rows": rows,
        "seedMs": seed_ms,
        "databasePath": str(db_path),
        "live": {
            "cold": stats(live_cold),
            "warm": stats(live_warm),
        },
        "cumulative": {
            "cold": stats(cumulative_cold),
            "warm": stats(cumulative_warm),
        },
        "explain": query_plan,
        "concurrency": concurrency,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rows", type=int, nargs="+", default=[100_000, 500_000])
    parser.add_argument("--warm-samples", type=int, default=20)
    parser.add_argument("--cold-samples", type=int, default=5)
    parser.add_argument("--writer-samples", type=int, default=100)
    parser.add_argument("--keep-db", action="store_true")
    args = parser.parse_args()

    temp_dir = Path(tempfile.mkdtemp(prefix="relay-dashboard-metrics-"))
    try:
        cases = [
            run_case(temp_dir, rows, args.warm_samples, args.cold_samples, args.writer_samples)
            for rows in args.rows
        ]
        print(json.dumps({"cases": cases}, indent=2, ensure_ascii=False))
    finally:
        if not args.keep_db:
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    main()
