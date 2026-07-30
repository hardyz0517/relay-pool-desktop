ALTER TABLE channel_monitors ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'generic_open_ai'
    CHECK (protocol_kind IN ('open_ai_chat', 'open_ai_responses', 'anthropic_messages', 'gemini_native', 'xai_grok', 'generic_open_ai'));
ALTER TABLE channel_monitors ADD COLUMN client_profile_id TEXT NOT NULL DEFAULT 'standard_api'
    CHECK (client_profile_id IN ('standard_api', 'codex_cli_compat', 'claude_code_compat', 'gemini_cli_compat', 'grok_cli_compat'));
ALTER TABLE channel_monitors ADD COLUMN client_profile_version INTEGER NOT NULL DEFAULT 1
    CHECK (client_profile_version >= 1);
ALTER TABLE channel_monitors ADD COLUMN primary_model TEXT NOT NULL DEFAULT 'gpt-4.1-mini'
    CHECK (trim(primary_model) <> '');
ALTER TABLE channel_monitors ADD COLUMN fallback_models_v2_json TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(fallback_models_v2_json) AND json_type(fallback_models_v2_json) = 'array');
ALTER TABLE channel_monitors ADD COLUMN retry_max_attempts_per_model INTEGER NOT NULL DEFAULT 1
    CHECK (retry_max_attempts_per_model BETWEEN 1 AND 3);
ALTER TABLE channel_monitors ADD COLUMN retry_initial_backoff_ms INTEGER NOT NULL DEFAULT 200
    CHECK (retry_initial_backoff_ms >= 0);
ALTER TABLE channel_monitors ADD COLUMN retry_max_backoff_ms INTEGER NOT NULL DEFAULT 2000
    CHECK (retry_max_backoff_ms >= retry_initial_backoff_ms);
ALTER TABLE channel_monitors ADD COLUMN risk_daily_probe_budget INTEGER NOT NULL DEFAULT 200
    CHECK (risk_daily_probe_budget BETWEEN 1 AND 10000);
ALTER TABLE channel_monitors ADD COLUMN health_writeback_mode TEXT NOT NULL DEFAULT 'observe_only'
    CHECK (health_writeback_mode IN ('disabled', 'observe_only', 'authoritative'));
ALTER TABLE channel_monitors ADD COLUMN health_failure_threshold INTEGER NOT NULL DEFAULT 2
    CHECK (health_failure_threshold BETWEEN 1 AND 20);
ALTER TABLE channel_monitors ADD COLUMN health_recovery_threshold INTEGER NOT NULL DEFAULT 2
    CHECK (health_recovery_threshold BETWEEN 1 AND 20);
ALTER TABLE channel_monitors ADD COLUMN attempt_timeout_ms INTEGER NOT NULL DEFAULT 10000
    CHECK (attempt_timeout_ms BETWEEN 1000 AND 120000);
ALTER TABLE channel_monitors ADD COLUMN execution_timeout_ms INTEGER NOT NULL DEFAULT 30000
    CHECK (execution_timeout_ms BETWEEN 1000 AND 300000 AND execution_timeout_ms > attempt_timeout_ms);
ALTER TABLE channel_monitors ADD COLUMN schedule_revision INTEGER NOT NULL DEFAULT 1
    CHECK (schedule_revision >= 1);
ALTER TABLE channel_monitors ADD COLUMN next_due_at_ms INTEGER
    CHECK (next_due_at_ms IS NULL OR next_due_at_ms >= 0);

UPDATE channel_monitors
SET primary_model = COALESCE(
        NULLIF(json_extract(fallback_models_json, '$[0]'), ''),
        primary_model
    ),
    fallback_models_v2_json = COALESCE(
        (
            SELECT json_group_array(value)
            FROM (
                SELECT DISTINCT json_each.value AS value
                FROM json_each(channel_monitors.fallback_models_json)
                WHERE json_each.key > 0
                  AND trim(json_each.value) <> ''
                  AND json_each.value <> COALESCE(NULLIF(json_extract(channel_monitors.fallback_models_json, '$[0]'), ''), channel_monitors.primary_model)
                LIMIT 3
            )
        ),
        '[]'
    ),
    next_due_at_ms = CASE
        WHEN next_run_at IS NOT NULL AND trim(next_run_at) <> '' THEN CAST(next_run_at AS INTEGER)
        ELSE NULL
    END,
    execution_timeout_ms = timeout_seconds * 1000,
    attempt_timeout_ms = CASE
        WHEN timeout_seconds * 1000 > 1000 THEN timeout_seconds * 1000 - 1000
        ELSE 1000
    END,
    health_failure_threshold = consecutive_failure_threshold;

CREATE INDEX idx_channel_monitors_v2_due
    ON channel_monitors(
        enabled,
        COALESCE(next_due_at_ms, 0) ASC,
        id ASC
    );

CREATE TABLE channel_monitor_executions (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('scheduled', 'manual', 'startup_recovery', 'legacy_import')),
    trigger_request_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'partial', 'cancelled', 'skipped', 'interrupted')),
    planned_at_ms INTEGER NOT NULL CHECK (planned_at_ms >= 0),
    started_at_ms INTEGER,
    finished_at_ms INTEGER,
    schedule_lag_ms INTEGER CHECK (schedule_lag_ms IS NULL OR schedule_lag_ms >= 0),
    config_revision INTEGER NOT NULL DEFAULT 1 CHECK (config_revision >= 1),
    config_snapshot_hash TEXT NOT NULL CHECK (trim(config_snapshot_hash) <> ''),
    endpoint_revision INTEGER NOT NULL DEFAULT 1 CHECK (endpoint_revision >= 1),
    target_count INTEGER NOT NULL DEFAULT 0 CHECK (target_count >= 0),
    available_count INTEGER NOT NULL DEFAULT 0 CHECK (available_count >= 0),
    degraded_count INTEGER NOT NULL DEFAULT 0 CHECK (degraded_count >= 0),
    unavailable_count INTEGER NOT NULL DEFAULT 0 CHECK (unavailable_count >= 0),
    skipped_count INTEGER NOT NULL DEFAULT 0 CHECK (skipped_count >= 0),
    summary_outcome TEXT CHECK (summary_outcome IS NULL OR summary_outcome IN ('available', 'degraded', 'unavailable', 'skipped')),
    summary_failure_kind TEXT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    CHECK (finished_at_ms IS NULL OR started_at_ms IS NULL OR finished_at_ms >= started_at_ms)
);

CREATE INDEX idx_channel_monitor_executions_monitor_started
    ON channel_monitor_executions(monitor_id, started_at_ms DESC, id DESC);

CREATE TABLE channel_monitor_attempts (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES channel_monitor_executions(id) ON DELETE CASCADE,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    endpoint_revision INTEGER NOT NULL DEFAULT 1 CHECK (endpoint_revision >= 1),
    model TEXT NOT NULL CHECK (trim(model) <> ''),
    model_role TEXT NOT NULL CHECK (model_role IN ('primary', 'fallback')),
    model_index INTEGER NOT NULL DEFAULT 0 CHECK (model_index >= 0),
    attempt_number INTEGER NOT NULL DEFAULT 0 CHECK (attempt_number >= 0),
    protocol_kind TEXT NOT NULL CHECK (protocol_kind IN ('open_ai_chat', 'open_ai_responses', 'anthropic_messages', 'gemini_native', 'xai_grok', 'generic_open_ai')),
    client_profile_id TEXT NOT NULL CHECK (trim(client_profile_id) <> ''),
    client_profile_version INTEGER NOT NULL CHECK (client_profile_version >= 1),
    request_profile_hash TEXT NOT NULL CHECK (trim(request_profile_hash) <> ''),
    transport_mode TEXT NOT NULL CHECK (transport_mode IN ('warm', 'cold_diagnostic')),
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
    headers_received_at_ms INTEGER,
    first_content_at_ms INTEGER,
    finished_at_ms INTEGER,
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    ttfb_ms INTEGER CHECK (ttfb_ms IS NULL OR ttfb_ms >= 0),
    first_content_ms INTEGER CHECK (first_content_ms IS NULL OR first_content_ms >= 0),
    http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
    outcome TEXT NOT NULL CHECK (outcome IN ('available', 'degraded', 'unavailable', 'skipped')),
    failure_kind TEXT,
    retryable INTEGER NOT NULL DEFAULT 0 CHECK (retryable IN (0, 1)),
    retry_after_ms INTEGER CHECK (retry_after_ms IS NULL OR retry_after_ms >= 0),
    response_model TEXT,
    content_extracted INTEGER NOT NULL DEFAULT 0 CHECK (content_extracted IN (0, 1)),
    validation_kind TEXT NOT NULL DEFAULT 'challenge',
    validation_passed INTEGER NOT NULL DEFAULT 0 CHECK (validation_passed IN (0, 1)),
    output_bytes INTEGER NOT NULL DEFAULT 0 CHECK (output_bytes >= 0),
    input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    total_tokens INTEGER CHECK (total_tokens IS NULL OR total_tokens >= 0),
    error_summary TEXT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (execution_id, station_key_id, model_role, model_index, attempt_number),
    CHECK (finished_at_ms IS NULL OR finished_at_ms >= started_at_ms)
);

CREATE INDEX idx_channel_monitor_attempts_execution
    ON channel_monitor_attempts(execution_id, station_key_id, model_role, model_index, attempt_number);

CREATE TABLE channel_monitor_target_results (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES channel_monitor_executions(id) ON DELETE CASCADE,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    endpoint_revision INTEGER NOT NULL DEFAULT 1 CHECK (endpoint_revision >= 1),
    terminal_outcome TEXT NOT NULL CHECK (terminal_outcome IN ('available', 'degraded', 'unavailable', 'skipped')),
    terminal_failure_kind TEXT,
    terminal_reason TEXT,
    requested_model TEXT NOT NULL CHECK (trim(requested_model) <> ''),
    effective_model TEXT,
    used_fallback INTEGER NOT NULL DEFAULT 0 CHECK (used_fallback IN (0, 1)),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    decisive_attempt_id TEXT REFERENCES channel_monitor_attempts(id) ON DELETE SET NULL,
    protocol_kind TEXT CHECK (protocol_kind IS NULL OR protocol_kind IN ('open_ai_chat', 'open_ai_responses', 'anthropic_messages', 'gemini_native', 'xai_grok', 'generic_open_ai')),
    resolved_adapter_kind TEXT NOT NULL CHECK (trim(resolved_adapter_kind) <> ''),
    resolved_dialect TEXT,
    client_profile_id TEXT NOT NULL CHECK (trim(client_profile_id) <> ''),
    client_profile_version INTEGER NOT NULL CHECK (client_profile_version >= 1),
    request_profile_hash TEXT CHECK (request_profile_hash IS NULL OR trim(request_profile_hash) <> ''),
    traffic_equivalence TEXT NOT NULL CHECK (traffic_equivalence IN ('standard_api', 'cli_compat', 'legacy_http_only')),
    health_writeback_mode TEXT NOT NULL CHECK (health_writeback_mode IN ('disabled', 'observe_only', 'authoritative')),
    health_writeback_decision TEXT NOT NULL CHECK (health_writeback_decision IN ('not_applicable', 'observe_only', 'write', 'suppressed')),
    health_writeback_reason TEXT,
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    ttfb_ms INTEGER CHECK (ttfb_ms IS NULL OR ttfb_ms >= 0),
    first_content_ms INTEGER CHECK (first_content_ms IS NULL OR first_content_ms >= 0),
    semantic_confidence TEXT NOT NULL CHECK (semantic_confidence IN ('protocol_validated', 'legacy_http_only')),
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
    finished_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (execution_id, station_key_id),
    CHECK (attempt_count > 0 OR terminal_outcome = 'skipped')
);

CREATE INDEX idx_channel_monitor_target_results_monitor_finished
    ON channel_monitor_target_results(monitor_id, finished_at_ms DESC, id DESC);

CREATE INDEX idx_channel_monitor_target_results_monitor_station_finished
    ON channel_monitor_target_results(monitor_id, station_key_id, finished_at_ms DESC, id DESC);

CREATE TABLE channel_monitor_bucket_rollups (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('hour', 'day')),
    bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
    bucket_end_ms INTEGER NOT NULL CHECK (bucket_end_ms > bucket_start_ms),
    total_count INTEGER NOT NULL DEFAULT 0 CHECK (total_count >= 0),
    available_count INTEGER NOT NULL DEFAULT 0 CHECK (available_count >= 0),
    degraded_count INTEGER NOT NULL DEFAULT 0 CHECK (degraded_count >= 0),
    unavailable_count INTEGER NOT NULL DEFAULT 0 CHECK (unavailable_count >= 0),
    skipped_count INTEGER NOT NULL DEFAULT 0 CHECK (skipped_count >= 0),
    failure_counts_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(failure_counts_json) AND json_type(failure_counts_json) = 'object'),
    p50_latency_ms INTEGER,
    p95_latency_ms INTEGER,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    UNIQUE (monitor_id, station_key_id, bucket_kind, bucket_start_ms)
);

CREATE TABLE channel_monitor_rollup_dirty_ranges (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    range_start_ms INTEGER NOT NULL CHECK (range_start_ms >= 0),
    range_end_ms INTEGER NOT NULL CHECK (range_end_ms >= range_start_ms),
    reason TEXT NOT NULL CHECK (trim(reason) <> ''),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TABLE station_key_health_observations (
    id TEXT PRIMARY KEY,
    station_key_id TEXT NOT NULL REFERENCES station_keys(id) ON DELETE CASCADE,
    target_result_id TEXT UNIQUE REFERENCES channel_monitor_target_results(id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('monitoring', 'proxy', 'manual')),
    source_event_id TEXT NOT NULL CHECK (trim(source_event_id) <> ''),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision >= 1),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'available', 'degraded', 'unavailable', 'skipped',
        'success', 'observe_failure', 'cooldown', 'hard_fail', 'neutral'
    )),
    failure_kind TEXT,
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    retry_after_ms INTEGER CHECK (retry_after_ms IS NULL OR retry_after_ms >= 0),
    traffic_equivalence TEXT NOT NULL DEFAULT 'unknown' CHECK (
        traffic_equivalence IN (
            'unknown', 'real_user_traffic', 'synthetic_standard',
            'synthetic_cli_compat', 'diagnostic'
        )
    ),
    error_summary TEXT,
    writeback_decision TEXT NOT NULL CHECK (writeback_decision IN ('not_applicable', 'observe_only', 'write', 'suppressed')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (source, source_event_id)
);

CREATE INDEX idx_station_key_health_observations_key_observed
    ON station_key_health_observations(station_key_id, observed_at_ms DESC, id DESC);

CREATE TABLE channel_monitor_probe_budget_usage (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    budget_window_start_ms INTEGER NOT NULL CHECK (budget_window_start_ms >= 0),
    budget_window_end_ms INTEGER NOT NULL CHECK (budget_window_end_ms > budget_window_start_ms),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    UNIQUE (monitor_id, station_key_id, budget_window_start_ms)
);

INSERT INTO channel_monitor_executions (
    id, monitor_id, trigger_kind, status, planned_at_ms, started_at_ms, finished_at_ms,
    config_snapshot_hash, target_count, available_count, degraded_count, unavailable_count,
    skipped_count, summary_outcome, summary_failure_kind, created_at_ms
)
SELECT
    'legacy-execution-' || id,
    monitor_id,
    'legacy_import',
    'completed',
    CAST(started_at AS INTEGER),
    CAST(started_at AS INTEGER),
    CAST(COALESCE(finished_at, started_at) AS INTEGER),
    'legacy-http-only',
    1,
    CASE WHEN status = 'success' THEN 1 ELSE 0 END,
    CASE WHEN status = 'warning' THEN 1 ELSE 0 END,
    CASE WHEN status = 'failed' THEN 1 ELSE 0 END,
    CASE WHEN status = 'skipped' THEN 1 ELSE 0 END,
    CASE status
        WHEN 'success' THEN 'available'
        WHEN 'warning' THEN 'degraded'
        WHEN 'failed' THEN 'unavailable'
        ELSE 'skipped'
    END,
    CASE status
        WHEN 'failed' THEN 'legacy_http_only'
        WHEN 'skipped' THEN 'needs_configuration'
        ELSE NULL
    END,
    CAST(created_at AS INTEGER)
FROM channel_monitor_runs;

INSERT INTO channel_monitor_attempts (
    id, execution_id, monitor_id, station_id, station_key_id, model, model_role, model_index,
    attempt_number, protocol_kind, client_profile_id, client_profile_version,
    request_profile_hash, transport_mode, started_at_ms, finished_at_ms, latency_ms,
    http_status, outcome, failure_kind, retryable, response_model, content_extracted,
    validation_passed, output_bytes, error_summary, created_at_ms
)
SELECT
    'legacy-attempt-' || r.id,
    'legacy-execution-' || r.id,
    r.monitor_id,
    r.station_id,
    r.station_key_id,
    COALESCE(r.fallback_model, m.primary_model),
    CASE WHEN r.fallback_model IS NULL THEN 'primary' ELSE 'fallback' END,
    CASE WHEN r.fallback_model IS NULL THEN 0 ELSE 1 END,
    0,
    m.protocol_kind,
    m.client_profile_id,
    m.client_profile_version,
    'legacy-http-only',
    'warm',
    CAST(r.started_at AS INTEGER),
    CAST(COALESCE(r.finished_at, r.started_at) AS INTEGER),
    r.latency_ms,
    r.http_status,
    CASE r.status
        WHEN 'success' THEN 'available'
        WHEN 'warning' THEN 'degraded'
        WHEN 'failed' THEN 'unavailable'
        ELSE 'skipped'
    END,
    CASE r.status
        WHEN 'failed' THEN 'legacy_http_only'
        WHEN 'skipped' THEN 'needs_configuration'
        ELSE NULL
    END,
    0,
    r.response_model,
    CASE WHEN r.status IN ('success', 'warning') THEN 1 ELSE 0 END,
    0,
    0,
    r.error_message,
    CAST(r.created_at AS INTEGER)
FROM channel_monitor_runs r
JOIN channel_monitors m ON m.id = r.monitor_id;

INSERT INTO channel_monitor_target_results (
    id, execution_id, monitor_id, station_id, station_key_id, terminal_outcome,
    terminal_failure_kind, terminal_reason, requested_model, effective_model, used_fallback,
    attempt_count, decisive_attempt_id, protocol_kind, resolved_adapter_kind,
    client_profile_id, client_profile_version, request_profile_hash, traffic_equivalence,
    health_writeback_mode, health_writeback_decision, health_writeback_reason, latency_ms,
    semantic_confidence, started_at_ms, finished_at_ms, created_at_ms
)
SELECT
    'legacy-target-' || r.id,
    'legacy-execution-' || r.id,
    r.monitor_id,
    r.station_id,
    r.station_key_id,
    CASE r.status
        WHEN 'success' THEN 'available'
        WHEN 'warning' THEN 'degraded'
        WHEN 'failed' THEN 'unavailable'
        ELSE 'skipped'
    END,
    CASE r.status
        WHEN 'failed' THEN 'legacy_http_only'
        WHEN 'skipped' THEN 'needs_configuration'
        ELSE NULL
    END,
    r.error_message,
    m.primary_model,
    COALESCE(r.fallback_model, r.response_model, m.primary_model),
    CASE WHEN r.fallback_model IS NULL THEN 0 ELSE 1 END,
    1,
    'legacy-attempt-' || r.id,
    m.protocol_kind,
    'legacy_http_only',
    m.client_profile_id,
    m.client_profile_version,
    'legacy-http-only',
    'legacy_http_only',
    'observe_only',
    'suppressed',
    'legacy_http_only_not_authoritative',
    r.latency_ms,
    'legacy_http_only',
    CAST(r.started_at AS INTEGER),
    CAST(COALESCE(r.finished_at, r.started_at) AS INTEGER),
    CAST(r.created_at AS INTEGER)
FROM channel_monitor_runs r
JOIN channel_monitors m ON m.id = r.monitor_id;

INSERT INTO channel_monitor_rollup_dirty_ranges (
    id, monitor_id, station_key_id, range_start_ms, range_end_ms, reason, created_at_ms
)
SELECT
    'legacy-dirty-' || r.id,
    r.monitor_id,
    r.station_key_id,
    CAST(r.started_at AS INTEGER),
    CAST(COALESCE(r.finished_at, r.started_at) AS INTEGER),
    'legacy_import',
    CAST(r.created_at AS INTEGER)
FROM channel_monitor_runs r;

UPDATE persistence_schema_compatibility
SET schema_version = 10,
    updated_by_migration = 10,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 9;

CREATE TEMP TABLE persistence_v10_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 10)
);

INSERT INTO persistence_v10_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v10_schema_guard;
