CREATE TABLE model_base_prices (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL CHECK (trim(provider) <> ''),
    model TEXT NOT NULL CHECK (trim(model) <> ''),
    input_price REAL CHECK (input_price IS NULL OR input_price >= 0),
    output_price REAL CHECK (output_price IS NULL OR output_price >= 0),
    currency TEXT NOT NULL CHECK (trim(currency) <> ''),
    unit TEXT NOT NULL CHECK (trim(unit) <> ''),
    source_url TEXT NOT NULL,
    source_label TEXT NOT NULL CHECK (trim(source_label) <> ''),
    source_checked_at TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    built_in INTEGER NOT NULL DEFAULT 0 CHECK (built_in IN (0, 1)),
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_model_base_prices_selection
    ON model_base_prices(model, enabled DESC, updated_at DESC, created_at DESC, id DESC);

CREATE TABLE channel_monitor_request_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (trim(name) <> ''),
    endpoint_kind TEXT NOT NULL CHECK (trim(endpoint_kind) <> ''),
    method TEXT NOT NULL CHECK (trim(method) <> ''),
    path TEXT NOT NULL CHECK (trim(path) <> ''),
    request_body_json TEXT NOT NULL
        CHECK (json_valid(request_body_json) AND json_type(request_body_json) = 'object'),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    built_in INTEGER NOT NULL DEFAULT 0 CHECK (built_in IN (0, 1)),
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_channel_monitor_templates_list
    ON channel_monitor_request_templates(enabled DESC, built_in DESC, updated_at DESC, id DESC);

CREATE TABLE channel_monitors (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (trim(name) <> ''),
    target_type TEXT NOT NULL CHECK (target_type IN ('station_key', 'station')),
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    template_id TEXT NOT NULL REFERENCES channel_monitor_request_templates(id),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    interval_seconds INTEGER NOT NULL CHECK (interval_seconds BETWEEN 15 AND 3600),
    jitter_seconds INTEGER NOT NULL DEFAULT 0 CHECK (jitter_seconds BETWEEN 0 AND 600),
    timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds BETWEEN 5 AND 120),
    max_concurrency INTEGER NOT NULL DEFAULT 1 CHECK (max_concurrency BETWEEN 1 AND 16),
    consecutive_failure_threshold INTEGER NOT NULL DEFAULT 3
        CHECK (consecutive_failure_threshold BETWEEN 1 AND 20),
    fallback_models_json TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(fallback_models_json) AND json_type(fallback_models_json) = 'array'),
    last_run_at TEXT,
    last_run_id TEXT,
    next_run_at TEXT,
    last_status TEXT CHECK (last_status IS NULL OR last_status IN ('success', 'warning', 'failed', 'skipped')),
    last_error_message TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (interval_seconds - jitter_seconds >= 15),
    CHECK (
        (target_type = 'station_key' AND station_key_id IS NOT NULL)
        OR (target_type = 'station' AND station_key_id IS NULL)
    )
);

CREATE INDEX idx_channel_monitors_list
    ON channel_monitors(enabled DESC, created_at ASC, id ASC);

CREATE INDEX idx_channel_monitors_template
    ON channel_monitors(template_id, id);

CREATE INDEX idx_channel_monitors_due
    ON channel_monitors(
        enabled,
        COALESCE(CAST(next_run_at AS INTEGER), 0) ASC,
        id ASC
    );

CREATE TABLE channel_monitor_runs (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES channel_monitors(id) ON DELETE CASCADE,
    template_id TEXT NOT NULL REFERENCES channel_monitor_request_templates(id),
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'warning', 'failed', 'skipped')),
    started_at TEXT NOT NULL CHECK (trim(started_at) <> ''),
    finished_at TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    response_model TEXT,
    fallback_model TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_channel_monitor_runs_monitor_started
    ON channel_monitor_runs(monitor_id, CAST(started_at AS INTEGER) DESC, id DESC);

CREATE INDEX idx_channel_monitor_runs_station_started
    ON channel_monitor_runs(station_id, station_key_id, CAST(started_at AS INTEGER) DESC, id DESC);

CREATE INDEX idx_pricing_rules_comparison
    ON pricing_rules(enabled DESC, station_id ASC, model ASC, updated_at DESC, created_at DESC, id DESC);

CREATE INDEX idx_pricing_rules_selection
    ON pricing_rules(station_id, model, enabled DESC, valid_from DESC, updated_at DESC, id DESC);

CREATE INDEX idx_balance_snapshots_latest_station_scope
    ON balance_snapshots(station_id, scope, updated_at DESC, created_at DESC, id DESC);

CREATE INDEX idx_group_rate_records_comparison
    ON group_rate_records(station_id, checked_at DESC, created_at DESC, id DESC);

CREATE INDEX idx_change_events_page
    ON change_events(updated_at DESC, id DESC);

CREATE INDEX idx_change_events_station_page
    ON change_events(station_id, updated_at DESC, id DESC);

UPDATE persistence_schema_compatibility
SET schema_version = 7,
    updated_by_migration = 7,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 6;

CREATE TEMP TABLE persistence_v7_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 7)
);

INSERT INTO persistence_v7_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v7_schema_guard;
