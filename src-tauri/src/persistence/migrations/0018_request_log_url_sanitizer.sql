CREATE TABLE IF NOT EXISTS request_log_url_sanitizer_progress (
    id TEXT PRIMARY KEY CHECK (id = 'request_logs_upstream_base_url_v1'),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete')),
    sanitized_count INTEGER NOT NULL DEFAULT 0 CHECK (sanitized_count >= 0),
    redacted_unparseable_count INTEGER NOT NULL DEFAULT 0 CHECK (redacted_unparseable_count >= 0),
    redacted_non_http_count INTEGER NOT NULL DEFAULT 0 CHECK (redacted_non_http_count >= 0),
    last_request_log_id TEXT,
    last_reason TEXT,
    updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO request_log_url_sanitizer_progress (
    id, status, sanitized_count, redacted_unparseable_count, redacted_non_http_count, updated_at
) VALUES (
    'request_logs_upstream_base_url_v1', 'pending', 0, 0, 0, CAST(strftime('%s','now') AS TEXT)
);

UPDATE persistence_schema_compatibility
SET schema_version = 18,
    updated_by_migration = 18,
    updated_at = CAST(strftime('%s','now') AS TEXT)
WHERE singleton_key = 1;
