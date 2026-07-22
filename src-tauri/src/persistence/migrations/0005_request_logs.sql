CREATE TABLE request_logs (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    model TEXT,
    stream INTEGER NOT NULL DEFAULT 0 CHECK (stream IN (0, 1)),
    status TEXT NOT NULL,
    lifecycle_status TEXT,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    station_id TEXT REFERENCES stations(id) ON DELETE SET NULL,
    upstream_base_url TEXT,
    fallback_count INTEGER NOT NULL DEFAULT 0 CHECK (fallback_count >= 0),
    error_message TEXT,
    route_policy TEXT,
    route_reason TEXT,
    rejected_candidates_json TEXT,
    body_bytes INTEGER,
    attempt_count INTEGER,
    route_wait_ms INTEGER,
    upstream_headers_ms INTEGER,
    failure_source TEXT,
    attempts_json TEXT,
    completion_source TEXT,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    total_tokens INTEGER,
    cache_creation_tokens INTEGER,
    cache_read_tokens INTEGER,
    reasoning_effort TEXT,
    first_token_ms INTEGER,
    terminal_kind TEXT,
    terminal_code TEXT,
    terminal_detail TEXT,
    protocol_completed INTEGER CHECK (protocol_completed IN (0, 1)),
    delivery_terminal TEXT,
    selected_attempt_ordinal INTEGER,
    terminal_at_ms INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_request_logs_created ON request_logs(created_at DESC, id DESC);

CREATE TABLE request_attempts (
    request_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    station_id TEXT NOT NULL,
    station_key_id TEXT NOT NULL,
    endpoint_revision INTEGER NOT NULL,
    started_at_ms INTEGER NOT NULL,
    terminal_kind TEXT NOT NULL,
    failure_kind TEXT,
    failure_blame TEXT,
    retry_disposition TEXT,
    health_effect TEXT NOT NULL,
    health_cooldown_until_ms INTEGER,
    public_code TEXT,
    sanitized_detail TEXT,
    output_committed INTEGER NOT NULL,
    terminal_at_ms INTEGER NOT NULL,
    PRIMARY KEY (request_id, ordinal),
    FOREIGN KEY(request_id) REFERENCES request_logs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_request_attempts_station_key_terminal
    ON request_attempts(station_key_id, terminal_at_ms DESC);

UPDATE persistence_schema_compatibility
SET schema_version = 5,
    updated_by_migration = 5,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
