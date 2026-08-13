-- A terminal write is first made durable here, then projected into request_logs
-- and request_routing_outcome_summaries.  The payload is the already-redacted,
-- canonical RequestTerminalWrite, never an upstream response or credentials.
CREATE TABLE request_terminal_outbox (
    request_id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL CHECK (length(payload_json) BETWEEN 2 AND 32768),
    payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64 AND payload_sha256 GLOB '[0-9a-f]*'),
    created_at_ms INTEGER NOT NULL,
    lease_owner TEXT,
    lease_expires_at_ms INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0)
);

CREATE INDEX idx_request_terminal_outbox_replay
    ON request_terminal_outbox(lease_expires_at_ms ASC, request_id ASC);

UPDATE persistence_schema_compatibility
SET schema_version = 39,
    updated_by_migration = 39,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 39;
