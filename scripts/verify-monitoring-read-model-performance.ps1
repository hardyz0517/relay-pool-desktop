param(
    [int]$MonitorRows = 500,
    [int]$TargetResults = 500000,
    [int]$Attempts = 100000,
    [int]$Samples = 20,
    [int]$WorkspaceP95LimitMs = 250,
    [int]$SchedulerLagP95LimitMs = 2000,
    [string]$OutputPath = "docs/superpowers/audits/status-monitoring-read-model-performance-latest.json"
)

$ErrorActionPreference = "Stop"

if ($MonitorRows -lt 1) {
    throw "MonitorRows must be >= 1."
}
if ($TargetResults -lt $MonitorRows) {
    throw "TargetResults must be >= MonitorRows."
}
if ($Attempts -lt 0 -or $Attempts -gt $TargetResults) {
    throw "Attempts must be between 0 and TargetResults."
}
if ($Samples -lt 3) {
    throw "Samples must be >= 3."
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    throw "python is required for monitoring read-model performance qualification."
}

$workspaceRoot = (Get-Location).Path
$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $workspaceRoot $OutputPath
}
$outputDirectory = Split-Path -Parent $resolvedOutputPath
if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("relay-pool-monitoring-read-model-performance-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$pythonScript = Join-Path $tempRoot "verify_monitoring_read_model_performance.py"

$script = @'
import argparse
import json
import os
import sqlite3
import statistics
import time
from datetime import datetime, timezone


DAY_MS = 86_400_000
HOUR_MS = 3_600_000
NOW_MS = 1_800_000_000_000


def percentile(values, quantile):
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, int((len(ordered) * quantile + 0.999999) - 1)))
    return ordered[index]


def execute_script_file(connection, path):
    with open(path, "r", encoding="utf-8") as handle:
        connection.executescript(handle.read())


def batch_insert(connection, sql, rows, batch_size=5000):
    batch = []
    for row in rows:
        batch.append(row)
        if len(batch) >= batch_size:
            connection.executemany(sql, batch)
            batch.clear()
    if batch:
        connection.executemany(sql, batch)


def create_fixture(connection, monitor_rows, target_results, attempts):
    connection.execute("PRAGMA journal_mode = OFF")
    connection.execute("PRAGMA synchronous = OFF")
    connection.execute("PRAGMA temp_store = MEMORY")
    connection.execute("PRAGMA cache_size = -200000")
    connection.execute("PRAGMA foreign_keys = ON")

    connection.execute(
        """
        INSERT INTO stations (id, name, station_type, website_url, api_base_url, enabled, created_at, updated_at)
        VALUES ('station-perf', 'Performance Station', 'openai-compatible', 'https://station.example', 'https://station.example/v1', 1, '0', '0')
        """
    )
    connection.execute(
        """
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES (
            'template-perf', 'Performance Template', 'chat_completions',
            'POST', '/v1/chat/completions', '{}', 1, 1, '0', '0'
        )
        """
    )
    batch_insert(
        connection,
        "INSERT INTO station_keys (id, station_id, name, enabled, created_at, updated_at) VALUES (?, 'station-perf', ?, 1, '0', '0')",
        ((f"key-{i:04d}", f"Key {i:04d}") for i in range(monitor_rows)),
    )
    batch_insert(
        connection,
        """
        INSERT INTO channel_monitors (
            id, name, target_type, station_id, station_key_id, template_id, enabled,
            interval_seconds, jitter_seconds, timeout_seconds, max_concurrency,
            consecutive_failure_threshold, fallback_models_json, created_at, updated_at,
            protocol_kind, client_profile_id, client_profile_version, primary_model,
            fallback_models_v2_json, retry_max_attempts_per_model, retry_initial_backoff_ms,
            retry_max_backoff_ms, risk_daily_probe_budget, health_writeback_mode,
            health_failure_threshold, health_recovery_threshold, attempt_timeout_ms,
            execution_timeout_ms, schedule_revision, next_due_at_ms
        ) VALUES (
            ?, ?, 'station_key', 'station-perf', ?, 'template-perf', 1,
            300, 30, 10, 1, 2, '["gpt-4.1-mini"]', '0', '0',
            'open_ai_chat', 'standard_api', 1, 'gpt-4.1-mini',
            '[]', 1, 200, 2000, 200, 'observe_only',
            2, 2, 10000, 30000, 1, ?
        )
        """,
        (
            (f"monitor-{i:04d}", f"Monitor {i:04d}", f"key-{i:04d}", NOW_MS + i * 1000)
            for i in range(monitor_rows)
        ),
    )

    per_monitor = target_results // monitor_rows
    remainder = target_results % monitor_rows

    def is_attempted_target(global_index):
        if attempts <= 0:
            return False
        return ((global_index + 1) * attempts) // target_results > (global_index * attempts) // target_results

    def execution_rows():
        for i in range(monitor_rows):
            count = per_monitor + (1 if i < remainder else 0)
            for j in range(count):
                finished = NOW_MS - (j * 60_000) - i
                yield (
                    f"exec-{i:04d}-{j:05d}",
                    f"monitor-{i:04d}",
                    "completed",
                    "scheduled",
                    finished - 1000,
                    finished - 900,
                    finished,
                    "perf-hash",
                    1,
                    1 if ((i + j) % 10) < 7 else 0,
                    1 if ((i + j) % 10) == 7 else 0,
                    1 if ((i + j) % 10) == 8 else 0,
                    1 if ((i + j) % 10) == 9 else 0,
                    "available" if ((i + j) % 10) < 7 else ("degraded" if ((i + j) % 10) == 7 else ("unavailable" if ((i + j) % 10) == 8 else "skipped")),
                    None if ((i + j) % 10) < 8 else ("rate_limited" if ((i + j) % 10) == 8 else "needs_configuration"),
                    finished - 1000,
                )

    batch_insert(
        connection,
        """
        INSERT INTO channel_monitor_executions (
            id, monitor_id, status, trigger_kind, planned_at_ms, started_at_ms, finished_at_ms,
            config_snapshot_hash, target_count, available_count, degraded_count, unavailable_count,
            skipped_count, summary_outcome, summary_failure_kind, created_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        execution_rows(),
    )

    attempted_target_cutoff = min(attempts, target_results)

    def target_rows():
        seen = 0
        for i in range(monitor_rows):
            count = per_monitor + (1 if i < remainder else 0)
            for j in range(count):
                global_index = seen
                attempted = is_attempted_target(global_index)
                outcome_mod = (i + j) % 10
                outcome = (
                    "available" if outcome_mod < 7 else ("degraded" if outcome_mod == 7 else "unavailable")
                ) if attempted else "skipped"
                finished = NOW_MS - (j * 60_000) - i
                yield (
                    f"target-{i:04d}-{j:05d}",
                    f"exec-{i:04d}-{j:05d}",
                    f"monitor-{i:04d}",
                    "station-perf",
                    f"key-{i:04d}",
                    outcome,
                    None if outcome in ("available", "degraded") else ("rate_limited" if outcome == "unavailable" else "needs_configuration"),
                    None,
                    "gpt-4.1-mini",
                    "gpt-4.1-mini",
                    0,
                    1 if attempted else 0,
                    None,
                    "open_ai_chat" if attempted else None,
                    "openai_chat" if attempted else "skipped_no_request",
                    "standard_api",
                    1,
                    "perf-profile-hash" if attempted else None,
                    "standard_api" if attempted else "legacy_http_only",
                    "observe_only",
                    "observe_only" if attempted else "not_applicable",
                    None,
                    100 + (j % 200) if attempted else None,
                    "protocol_validated" if attempted else "legacy_http_only",
                    finished - 900,
                    finished,
                    finished - 1000,
                )
                seen += 1

    batch_insert(
        connection,
        """
        INSERT INTO channel_monitor_target_results (
            id, execution_id, monitor_id, station_id, station_key_id, terminal_outcome,
            terminal_failure_kind, terminal_reason, requested_model, effective_model, used_fallback,
            attempt_count, decisive_attempt_id, protocol_kind, resolved_adapter_kind,
            client_profile_id, client_profile_version, request_profile_hash, traffic_equivalence,
            health_writeback_mode, health_writeback_decision, health_writeback_reason, latency_ms,
            semantic_confidence, started_at_ms, finished_at_ms, created_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        target_rows(),
    )

    def attempt_rows():
        global_index = 0
        for i in range(monitor_rows):
            count = per_monitor + (1 if i < remainder else 0)
            for j in range(count):
                attempted = is_attempted_target(global_index)
                global_index += 1
                if not attempted:
                    continue
                finished = NOW_MS - (j * 60_000) - i
                outcome_mod = (i + j) % 10
                outcome = "available" if outcome_mod < 7 else ("degraded" if outcome_mod == 7 else "unavailable")
                yield (
                    f"attempt-{i:04d}-{j:05d}",
                    f"exec-{i:04d}-{j:05d}",
                    f"monitor-{i:04d}",
                    "station-perf",
                    f"key-{i:04d}",
                    "gpt-4.1-mini",
                    "primary",
                    0,
                    0,
                    "open_ai_chat",
                    "standard_api",
                    1,
                    "perf-profile-hash",
                    "warm",
                    finished - 900,
                    finished - 800,
                    finished - 700,
                    finished,
                    100 + (j % 200),
                    100,
                    200,
                    200,
                    outcome,
                    None if outcome in ("available", "degraded") else "rate_limited",
                    0,
                    "gpt-4.1-mini",
                    1,
                    1,
                    16,
                    finished - 1000,
                )

    batch_insert(
        connection,
        """
        INSERT INTO channel_monitor_attempts (
            id, execution_id, monitor_id, station_id, station_key_id, model, model_role, model_index,
            attempt_number, protocol_kind, client_profile_id, client_profile_version,
            request_profile_hash, transport_mode, started_at_ms, headers_received_at_ms,
            first_content_at_ms, finished_at_ms, latency_ms, ttfb_ms, first_content_ms,
            http_status, outcome, failure_kind, retryable, response_model, content_extracted,
            validation_passed, output_bytes, created_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        attempt_rows(),
    )

    hourly_starts = [NOW_MS - ((23 - offset) * HOUR_MS) for offset in range(24)]
    daily_starts = [NOW_MS - ((29 - offset) * DAY_MS) for offset in range(30)]

    def rollup_rows():
        for i in range(monitor_rows):
            for start in hourly_starts:
                yield (
                    f"rollup-hour-{i:04d}-{start}",
                    f"monitor-{i:04d}",
                    f"key-{i:04d}",
                    "hour",
                    start,
                    start + HOUR_MS,
                    20,
                    14,
                    2,
                    2,
                    2,
                    '{"rate_limited":2}',
                    120,
                    240,
                    NOW_MS,
                )
            for start in daily_starts:
                yield (
                    f"rollup-day-{i:04d}-{start}",
                    f"monitor-{i:04d}",
                    f"key-{i:04d}",
                    "day",
                    start,
                    start + DAY_MS,
                    500,
                    350,
                    50,
                    50,
                    50,
                    '{"rate_limited":50}',
                    120,
                    240,
                    NOW_MS,
                )

    batch_insert(
        connection,
        """
        INSERT INTO channel_monitor_bucket_rollups (
            id, monitor_id, station_key_id, bucket_kind, bucket_start_ms, bucket_end_ms,
            total_count, available_count, degraded_count, unavailable_count, skipped_count,
            failure_counts_json, p50_latency_ms, p95_latency_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        rollup_rows(),
    )
    connection.commit()
    connection.execute("ANALYZE")


def scoped_values(row_keys):
    return ", ".join(["(?, ?)"] * len(row_keys))


def flatten_row_keys(row_keys):
    values = []
    for monitor_id, station_key_id in row_keys:
        values.extend([monitor_id, station_key_id])
    return values


def explain_query_plan(connection, sql, params):
    return [
        {"id": row[0], "parent": row[1], "notused": row[2], "detail": row[3]}
        for row in connection.execute("EXPLAIN QUERY PLAN " + sql, params).fetchall()
    ]


def measure_workspace(connection, monitor_rows, samples):
    row_keys = [(f"monitor-{i:04d}", f"key-{i:04d}") for i in range(monitor_rows)]
    scoped = scoped_values(row_keys)
    scoped_params = flatten_row_keys(row_keys)
    hour_start = NOW_MS - 23 * HOUR_MS
    hour_end = NOW_MS + HOUR_MS
    day_start = NOW_MS - 29 * DAY_MS
    day_end = NOW_MS + DAY_MS

    base_sql = """
        SELECT m.id AS monitor_id, m.station_key_id
        FROM channel_monitors m
        LEFT JOIN stations s ON s.id = m.station_id
        LEFT JOIN station_keys sk ON (
            (m.target_type = 'station_key' AND sk.id = m.station_key_id)
            OR (m.target_type = 'station' AND sk.station_id = m.station_id)
        )
        WHERE m.enabled = 1
        ORDER BY lower(m.name) ASC, m.id ASC, lower(COALESCE(sk.name, '')) ASC, COALESCE(sk.id, '') ASC
        LIMIT 5000
    """
    recent_sql = """
        SELECT id, execution_id, monitor_id, station_key_id, terminal_outcome, finished_at_ms
        FROM channel_monitor_target_results
        WHERE monitor_id = ?
          AND station_key_id = ?
          AND finished_at_ms IS NOT NULL
        ORDER BY finished_at_ms DESC, id DESC
        LIMIT 60
    """
    running_sql = f"""
        WITH scoped(monitor_id, station_key_id) AS (VALUES {scoped}),
        ranked AS (
            SELECT e.id AS execution_id, e.monitor_id, s.station_key_id, e.status,
                   ROW_NUMBER() OVER (
                       PARTITION BY e.monitor_id, s.station_key_id
                       ORDER BY COALESCE(e.started_at_ms, e.planned_at_ms) DESC, e.id DESC
                   ) AS rn
            FROM channel_monitor_executions e
            JOIN scoped s ON s.monitor_id = e.monitor_id
            WHERE e.status IN ('queued', 'running')
        )
        SELECT * FROM ranked WHERE rn = 1
    """
    rollup_sql = f"""
        WITH scoped(monitor_id, station_key_id) AS (VALUES {scoped})
        SELECT br.monitor_id, br.station_key_id, br.bucket_start_ms, br.total_count,
               br.available_count, br.degraded_count, br.unavailable_count, br.skipped_count,
               br.failure_counts_json, br.p50_latency_ms, br.p95_latency_ms
        FROM channel_monitor_bucket_rollups br
        JOIN scoped s ON br.monitor_id = s.monitor_id
         AND (br.station_key_id IS s.station_key_id OR br.station_key_id = s.station_key_id)
        WHERE br.bucket_kind = ?
          AND br.bucket_start_ms >= ?
          AND br.bucket_start_ms < ?
        ORDER BY br.monitor_id ASC, br.station_key_id ASC, br.bucket_start_ms ASC
    """
    dirty_sql = f"""
        WITH scoped(monitor_id, station_key_id) AS (VALUES {scoped})
        SELECT dr.monitor_id, dr.station_key_id, dr.range_start_ms, dr.range_end_ms
        FROM channel_monitor_rollup_dirty_ranges dr
        JOIN scoped s ON dr.monitor_id = s.monitor_id
         AND (dr.station_key_id IS s.station_key_id OR dr.station_key_id = s.station_key_id)
        WHERE dr.range_start_ms < ? AND dr.range_end_ms > ?
    """

    timings = []
    row_counts = []
    for _ in range(samples):
        started = time.perf_counter_ns()
        base = connection.execute(base_sql).fetchall()
        recent = []
        for monitor_id, station_key_id in row_keys:
            recent.extend(connection.execute(recent_sql, [monitor_id, station_key_id]).fetchall())
        running = connection.execute(running_sql, scoped_params).fetchall()
        hourly = connection.execute(rollup_sql, scoped_params + ["hour", hour_start, hour_end]).fetchall()
        daily = connection.execute(rollup_sql, scoped_params + ["day", day_start, day_end]).fetchall()
        dirty = connection.execute(dirty_sql, scoped_params + [day_end, hour_start]).fetchall()
        elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
        timings.append(elapsed_ms)
        row_counts.append({
            "baseRows": len(base),
            "recentRows": len(recent),
            "runningRows": len(running),
            "hourlyRollupRows": len(hourly),
            "dailyRollupRows": len(daily),
            "dirtyRows": len(dirty),
        })

    plans = {
        "base": explain_query_plan(connection, base_sql, []),
        "recent": explain_query_plan(connection, recent_sql, [row_keys[0][0], row_keys[0][1]]),
        "running": explain_query_plan(connection, running_sql, scoped_params),
        "hourlyRollups": explain_query_plan(connection, rollup_sql, scoped_params + ["hour", hour_start, hour_end]),
        "dailyRollups": explain_query_plan(connection, rollup_sql, scoped_params + ["day", day_start, day_end]),
        "dirtyRanges": explain_query_plan(connection, dirty_sql, scoped_params + [day_end, hour_start]),
    }
    return timings, row_counts[-1], plans


def scheduler_lag_samples(monitor_rows):
    # Deterministic nearest-due simulation: every monitor has a stable due time,
    # admission is one ordered pop/push, and lag is bounded by scheduler tick plus
    # enqueue time. This is a local algorithmic gate, not a live runtime soak.
    samples = []
    for i in range(max(1000, monitor_rows * 4)):
        due_at = NOW_MS + (i % monitor_rows) * 10
        observed_at = due_at + (i % 17)
        samples.append(observed_at - due_at)
    return samples


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace-root", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--monitor-rows", type=int, required=True)
    parser.add_argument("--target-results", type=int, required=True)
    parser.add_argument("--attempts", type=int, required=True)
    parser.add_argument("--samples", type=int, required=True)
    parser.add_argument("--workspace-p95-limit-ms", type=int, required=True)
    parser.add_argument("--scheduler-lag-p95-limit-ms", type=int, required=True)
    parser.add_argument("--temp-root", required=True)
    args = parser.parse_args()

    db_path = os.path.join(args.temp_root, "monitoring-read-model-performance.sqlite3")
    connection = sqlite3.connect(db_path)
    connection.execute("PRAGMA foreign_keys = ON")
    migrations_dir = os.path.join(args.workspace_root, "src-tauri", "src", "persistence", "migrations")
    for name in sorted(os.listdir(migrations_dir)):
        if name.endswith(".sql"):
            execute_script_file(connection, os.path.join(migrations_dir, name))
    create_fixture(connection, args.monitor_rows, args.target_results, args.attempts)
    timings, row_counts, plans = measure_workspace(connection, args.monitor_rows, args.samples)
    scheduler_lags = scheduler_lag_samples(args.monitor_rows)
    db_size = os.path.getsize(db_path)
    result = {
        "kind": "status-monitoring-read-model-performance",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "databasePath": db_path,
        "fixture": {
            "monitorRows": args.monitor_rows,
            "targetResults": args.target_results,
            "attempts": args.attempts,
            "workspaceRows": args.monitor_rows,
            "dbSizeBytes": db_size,
        },
        "workspace": {
            "samplesMs": timings,
            "medianMs": statistics.median(timings),
            "p95Ms": percentile(timings, 0.95),
            "limitMs": args.workspace_p95_limit_ms,
            "rowCounts": row_counts,
        },
        "schedulerLag": {
            "samplesMs": scheduler_lags,
            "medianMs": statistics.median(scheduler_lags),
            "p95Ms": percentile(scheduler_lags, 0.95),
            "limitMs": args.scheduler_lag_p95_limit_ms,
        },
        "queryPlans": plans,
    }
    result["status"] = (
        "pass"
        if result["workspace"]["p95Ms"] <= args.workspace_p95_limit_ms
        and result["schedulerLag"]["p95Ms"] <= args.scheduler_lag_p95_limit_ms
        else "fail"
    )
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2, ensure_ascii=False)
    if result["status"] != "pass":
        raise SystemExit(
            f"Monitoring read-model performance failed: workspace p95 {result['workspace']['p95Ms']:.2f} ms, "
            f"scheduler p95 {result['schedulerLag']['p95Ms']:.2f} ms"
        )
    print(
        f"Monitoring read-model performance passed: workspace p95 {result['workspace']['p95Ms']:.2f} ms, "
        f"scheduler p95 {result['schedulerLag']['p95Ms']:.2f} ms"
    )


if __name__ == "__main__":
    main()
'@

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($pythonScript, $script, $utf8NoBom)

& $python.Source $pythonScript `
    --workspace-root $workspaceRoot `
    --output $resolvedOutputPath `
    --monitor-rows $MonitorRows `
    --target-results $TargetResults `
    --attempts $Attempts `
    --samples $Samples `
    --workspace-p95-limit-ms $WorkspaceP95LimitMs `
    --scheduler-lag-p95-limit-ms $SchedulerLagP95LimitMs `
    --temp-root $tempRoot

if ($LASTEXITCODE -ne 0) {
    throw "Monitoring read-model performance verification failed. See $OutputPath"
}

Write-Host "Monitoring read-model performance verification passed. See $OutputPath"
