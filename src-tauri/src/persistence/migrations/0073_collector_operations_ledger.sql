-- Durable collector operation history.  This is append-oriented history and
-- deliberately contains no credential material or provider payloads.
CREATE TABLE collector_operations (
    operation_id TEXT PRIMARY KEY,
    operation_key TEXT NOT NULL UNIQUE,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    intent_sequence INTEGER NOT NULL CHECK (intent_sequence > 0),
    plan_version TEXT NOT NULL CHECK (length(plan_version) BETWEEN 1 AND 64),
    task_type TEXT NOT NULL CHECK (length(task_type) BETWEEN 1 AND 64),
    trigger_kind TEXT NOT NULL CHECK (length(trigger_kind) BETWEEN 1 AND 64),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'partially_succeeded', 'failed', 'cancelled', 'interrupted', 'superseded')),
    started_at_ms INTEGER CHECK (started_at_ms IS NULL OR started_at_ms >= 0),
    finished_at_ms INTEGER CHECK (finished_at_ms IS NULL OR finished_at_ms >= 0),
    reason_code TEXT CHECK (reason_code IS NULL OR length(reason_code) <= 128),
    reason_detail TEXT CHECK (reason_detail IS NULL OR length(reason_detail) <= 512),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE INDEX idx_collector_operations_station_created
    ON collector_operations(station_id, created_at_ms DESC, operation_id DESC);
CREATE INDEX idx_collector_operations_due
    ON collector_operations(status, updated_at_ms, station_id);

ALTER TABLE post_authorization_collection_work RENAME TO post_authorization_collection_work_v72;
DROP INDEX idx_post_authorization_work_due;

CREATE TABLE post_authorization_collection_work (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'superseded', 'exhausted')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts BETWEEN 1 AND 16),
    next_attempt_at_ms INTEGER NOT NULL CHECK (next_attempt_at_ms >= 0),
    last_error_code TEXT CHECK (last_error_code IS NULL OR length(last_error_code) <= 128),
    operation_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

INSERT INTO post_authorization_collection_work (
    station_id, endpoint_revision, credential_revision, state, attempt_count,
    max_attempts, next_attempt_at_ms, last_error_code, operation_id,
    created_at_ms, updated_at_ms
)
SELECT station_id, endpoint_revision, credential_revision, state, attempt_count,
       5, next_attempt_at_ms, substr(last_error_code, 1, 128), operation_id,
       created_at_ms, updated_at_ms
FROM post_authorization_collection_work_v72;

DROP TABLE post_authorization_collection_work_v72;

CREATE INDEX idx_post_authorization_work_due
    ON post_authorization_collection_work(state, next_attempt_at_ms, station_id);

UPDATE persistence_schema_compatibility
SET schema_version = 73,
    updated_by_migration = 73,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 73;

CREATE TEMP TABLE persistence_v73_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 73)
);
INSERT INTO persistence_v73_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v73_schema_guard;
