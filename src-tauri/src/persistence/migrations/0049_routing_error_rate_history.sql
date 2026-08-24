-- Bounded, low-cardinality diagnostic history for the durable health reducer.
-- Scope values are one-way commitments; this table must never contain raw
-- credentials, URLs, request IDs, model names, or upstream error text.
CREATE TABLE routing_error_rate_history (
    ingestion_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id TEXT NOT NULL UNIQUE CHECK (length(observation_id) BETWEEN 1 AND 160),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'credential', 'account', 'group', 'endpoint', 'model', 'capacity_domain'
    )),
    scope_commitment TEXT NOT NULL CHECK (
        length(scope_commitment) = 64
        AND scope_commitment NOT GLOB '*[^0-9a-fA-F]*'
    ),
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failure')),
    failure_code TEXT CHECK (failure_code IS NULL OR failure_code IN (
        'connect_failure', 'first_byte_timeout', 'upstream_5xx',
        'rate_limited', 'capacity_exhausted', 'endpoint_unavailable', 'unknown'
    )),
    sample_count INTEGER NOT NULL CHECK (sample_count > 0),
    failure_count INTEGER NOT NULL CHECK (failure_count BETWEEN 0 AND sample_count),
    failure_rate_percent INTEGER NOT NULL CHECK (failure_rate_percent BETWEEN 0 AND 100),
    transition TEXT CHECK (transition IS NULL OR transition IN (
        'ignored_duplicate', 'observed', 'opened', 'probe_succeeded',
        'closed', 'reopened'
    )),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE INDEX idx_routing_error_rate_history_timeline
    ON routing_error_rate_history(observed_at_ms ASC, ingestion_sequence ASC);
CREATE INDEX idx_routing_error_rate_history_scope
    ON routing_error_rate_history(scope_kind, scope_commitment, observed_at_ms ASC);

CREATE TABLE routing_error_rate_history_meta (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    dropped_events INTEGER NOT NULL DEFAULT 0 CHECK (dropped_events >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

INSERT INTO routing_error_rate_history_meta (singleton_key, dropped_events, updated_at_ms)
VALUES (1, 0, 0);

UPDATE persistence_schema_compatibility
SET schema_version = 49,
    updated_by_migration = 49,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version = 48;

CREATE TEMP TABLE persistence_v49_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 49)
);
INSERT INTO persistence_v49_schema_guard (schema_version)
SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1;
DROP TABLE persistence_v49_schema_guard;
