ALTER TABLE request_logs ADD COLUMN received_at_ms INTEGER;

ALTER TABLE request_logs ADD COLUMN usage_status TEXT NOT NULL DEFAULT 'unknown_legacy'
    CHECK (usage_status IN (
        'in_progress',
        'complete',
        'missing_usage',
        'stream_usage_missing',
        'not_applicable',
        'unknown_legacy'
    ));

UPDATE request_logs
SET received_at_ms = CAST(started_at AS INTEGER)
WHERE received_at_ms IS NULL
  AND trim(started_at) <> ''
  AND trim(started_at) NOT GLOB '*[^0-9]*'
  AND CAST(started_at AS INTEGER) > 0;

UPDATE request_logs
SET received_at_ms = CAST((julianday(started_at) - 2440587.5) * 86400000 AS INTEGER)
WHERE received_at_ms IS NULL
  AND trim(started_at) <> ''
  AND trim(started_at) GLOB '*[-T:Z+]*'
  AND julianday(started_at) IS NOT NULL;

UPDATE request_logs
SET usage_status = CASE
    WHEN terminal_at_ms IS NULL OR status = 'in_progress' THEN 'in_progress'
    WHEN lower(endpoint) LIKE '%models%'
      OR lower(endpoint) LIKE '%usage%'
      OR lower(endpoint) LIKE '%embeddings%' THEN 'not_applicable'
    WHEN total_tokens IS NOT NULL THEN 'complete'
    WHEN stream = 1 THEN 'stream_usage_missing'
    ELSE 'missing_usage'
END;

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

CREATE INDEX idx_request_logs_terminal_received_at
    ON request_logs(received_at_ms DESC)
    WHERE terminal_at_ms IS NOT NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 22,
    updated_by_migration = 22,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 22;
