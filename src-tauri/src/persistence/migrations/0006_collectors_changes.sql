CREATE TABLE collector_runs (
    id TEXT PRIMARY KEY,
    run_key TEXT NOT NULL UNIQUE,
    request_hash TEXT NOT NULL,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL,
    parent_run_id TEXT REFERENCES collector_runs(id) ON DELETE SET NULL,
    adapter TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    endpoint_count INTEGER NOT NULL DEFAULT 0 CHECK (endpoint_count >= 0),
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    manual_action_required INTEGER NOT NULL DEFAULT 0 CHECK (manual_action_required IN (0, 1)),
    error_code TEXT,
    error_message TEXT,
    snapshot_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_collector_runs_station_created
    ON collector_runs(station_id, created_at DESC, id DESC);
CREATE INDEX idx_collector_runs_parent ON collector_runs(parent_run_id);

CREATE TABLE collector_snapshots (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL UNIQUE REFERENCES collector_runs(id) ON DELETE CASCADE,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL,
    source TEXT NOT NULL,
    status TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    normalized_json TEXT NOT NULL CHECK (json_valid(normalized_json)),
    raw_json_redacted TEXT CHECK (raw_json_redacted IS NULL OR json_valid(raw_json_redacted)),
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_collector_snapshots_station_created
    ON collector_snapshots(station_id, created_at DESC, id DESC);

CREATE TABLE station_group_bindings (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    binding_kind TEXT NOT NULL CHECK (binding_kind IN ('station_group', 'key_binding')),
    parent_group_binding_id TEXT REFERENCES station_group_bindings(id) ON DELETE SET NULL,
    group_key_hash TEXT NOT NULL,
    group_id_hash TEXT,
    group_name TEXT NOT NULL,
    binding_status TEXT NOT NULL CHECK (binding_status IN ('available', 'bound', 'missing', 'disabled', 'manual_legacy')),
    default_rate_multiplier REAL,
    user_rate_multiplier REAL,
    effective_rate_multiplier REAL,
    inferred_group_category TEXT,
    group_category_override TEXT,
    rate_source TEXT,
    confidence REAL NOT NULL DEFAULT 0.5 CHECK (confidence >= 0 AND confidence <= 1),
    last_seen_at TEXT,
    last_checked_at TEXT,
    last_rate_changed_at TEXT,
    last_seen_run_id TEXT REFERENCES collector_runs(id) ON DELETE SET NULL,
    raw_json_redacted TEXT CHECK (raw_json_redacted IS NULL OR json_valid(raw_json_redacted)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((binding_kind = 'station_group' AND station_key_id IS NULL)
        OR (binding_kind = 'key_binding' AND station_key_id IS NOT NULL))
);

CREATE UNIQUE INDEX idx_group_bindings_station_group_key
    ON station_group_bindings(station_id, binding_kind, group_key_hash)
    WHERE binding_kind = 'station_group';
CREATE UNIQUE INDEX idx_group_bindings_key_group_key
    ON station_group_bindings(station_key_id, binding_kind, group_key_hash)
    WHERE binding_kind = 'key_binding';
CREATE INDEX idx_group_bindings_station_status
    ON station_group_bindings(station_id, binding_status, updated_at DESC);

CREATE TABLE group_rate_records (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    group_binding_id TEXT REFERENCES station_group_bindings(id) ON DELETE SET NULL,
    binding_kind TEXT NOT NULL,
    group_key_hash TEXT NOT NULL,
    group_name TEXT NOT NULL,
    default_rate_multiplier REAL,
    user_rate_multiplier REAL,
    effective_rate_multiplier REAL,
    inferred_group_category TEXT,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5 CHECK (confidence >= 0 AND confidence <= 1),
    raw_json_redacted TEXT CHECK (raw_json_redacted IS NULL OR json_valid(raw_json_redacted)),
    checked_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_group_rate_records_binding_checked
    ON group_rate_records(group_binding_id, checked_at DESC, id DESC);
CREATE INDEX idx_group_rate_records_station_checked
    ON group_rate_records(station_id, checked_at DESC, id DESC);

CREATE TABLE collector_model_facts (
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    available INTEGER NOT NULL CHECK (available IN (0, 1)),
    source TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    last_seen_run_id TEXT NOT NULL REFERENCES collector_runs(id) ON DELETE CASCADE,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (station_id, model)
);

CREATE TABLE collector_task_state (
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL,
    last_run_id TEXT NOT NULL REFERENCES collector_runs(id) ON DELETE CASCADE,
    last_status TEXT NOT NULL,
    last_success_at TEXT,
    last_failure_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    next_due_at TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (station_id, task_type)
);

CREATE INDEX idx_collector_task_state_due
    ON collector_task_state(next_due_at, station_id, task_type);

CREATE TABLE change_events (
    id TEXT PRIMARY KEY,
    severity TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('unread', 'read', 'dismissed', 'resolved')),
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    object_type TEXT NOT NULL,
    object_id TEXT,
    station_id TEXT REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    pricing_rule_id TEXT REFERENCES pricing_rules(id) ON DELETE SET NULL,
    request_log_id TEXT REFERENCES request_logs(id) ON DELETE SET NULL,
    old_value_json TEXT CHECK (old_value_json IS NULL OR json_valid(old_value_json)),
    new_value_json TEXT CHECK (new_value_json IS NULL OR json_valid(new_value_json)),
    impact_json TEXT CHECK (impact_json IS NULL OR json_valid(impact_json)),
    dedupe_key TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL,
    detected_at TEXT NOT NULL,
    resolved_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_change_events_status_severity_updated
    ON change_events(status, severity, updated_at DESC, id DESC);
CREATE INDEX idx_change_events_station_updated
    ON change_events(station_id, updated_at DESC, id DESC);
CREATE INDEX idx_change_events_station_key_updated
    ON change_events(station_key_id, updated_at DESC, id DESC);

UPDATE persistence_schema_compatibility
SET schema_version = 6,
    updated_by_migration = 6,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
