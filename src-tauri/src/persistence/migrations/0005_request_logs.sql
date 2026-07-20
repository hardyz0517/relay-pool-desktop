CREATE TABLE IF NOT EXISTS request_attempts (
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
