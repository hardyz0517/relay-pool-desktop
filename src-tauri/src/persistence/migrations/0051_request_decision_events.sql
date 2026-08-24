-- Bounded, redacted request decision facts.  This is a durable projection of
-- lifecycle milestones, not a copy of the in-memory runtime trace.  Event
-- keys make retries/idempotent lifecycle writes safe while sequence preserves
-- the order in which facts became durable.
CREATE TABLE request_decision_events (
    request_id TEXT NOT NULL REFERENCES request_logs(id) ON DELETE CASCADE,
    event_key TEXT NOT NULL CHECK (
        length(event_key) BETWEEN 1 AND 96
        AND event_key NOT GLOB '*[^a-z0-9_:-]*'
    ),
    sequence INTEGER NOT NULL CHECK (sequence >= 0 AND sequence < 64),
    occurred_at_ms INTEGER NOT NULL,
    event_kind TEXT NOT NULL CHECK (
        length(event_kind) BETWEEN 1 AND 48
        AND event_kind NOT GLOB '*[^a-z0-9_:-]*'
    ),
    detail_code TEXT NOT NULL CHECK (
        length(detail_code) BETWEEN 1 AND 96
        AND detail_code NOT GLOB '*[^a-z0-9_:-]*'
    ),
    attempt_ordinal INTEGER CHECK (attempt_ordinal IS NULL OR (attempt_ordinal >= 0 AND attempt_ordinal < 16)),
    retry_disposition TEXT CHECK (
        retry_disposition IS NULL
        OR (
            length(retry_disposition) BETWEEN 1 AND 48
            AND retry_disposition NOT GLOB '*[^a-z0-9_:-]*'
        )
    ),
    output_committed INTEGER CHECK (output_committed IS NULL OR output_committed IN (0, 1)),
    PRIMARY KEY (request_id, event_key),
    UNIQUE (request_id, sequence)
);

CREATE INDEX idx_request_decision_events_timeline
    ON request_decision_events(request_id, sequence ASC);

UPDATE persistence_schema_compatibility
SET schema_version = 51,
    updated_by_migration = 51,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 51;
