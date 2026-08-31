-- Bounded retention metadata for routing v3 raw observations and circuit
-- events. The projector task remains the single retention owner; these tables
-- retain only aggregate counts and cursor ranges, never key identities or raw
-- payloads.

CREATE TABLE routing_raw_event_retention_rollup (
    event_kind TEXT NOT NULL CHECK (event_kind IN ('observation', 'circuit')),
    source_kind TEXT NOT NULL CHECK (length(source_kind) BETWEEN 1 AND 64),
    outcome_kind TEXT NOT NULL CHECK (length(outcome_kind) BETWEEN 1 AND 64),
    bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
    deleted_count INTEGER NOT NULL CHECK (deleted_count > 0),
    first_ingestion_sequence INTEGER NOT NULL CHECK (first_ingestion_sequence > 0),
    last_ingestion_sequence INTEGER NOT NULL CHECK (
        last_ingestion_sequence >= first_ingestion_sequence
    ),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= bucket_start_ms),
    PRIMARY KEY (event_kind, source_kind, outcome_kind, bucket_start_ms)
);

CREATE TABLE routing_raw_event_retention_run (
    run_id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
    finished_at_ms INTEGER NOT NULL CHECK (finished_at_ms >= started_at_ms),
    cutoff_at_ms INTEGER NOT NULL CHECK (cutoff_at_ms >= 0),
    observation_safe_sequence INTEGER NOT NULL CHECK (observation_safe_sequence >= 0),
    circuit_safe_sequence INTEGER NOT NULL CHECK (circuit_safe_sequence >= 0),
    observations_deleted INTEGER NOT NULL CHECK (observations_deleted >= 0),
    circuit_events_deleted INTEGER NOT NULL CHECK (circuit_events_deleted >= 0),
    status TEXT NOT NULL CHECK (status IN ('succeeded', 'failed')),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 96),
    CHECK ((status = 'succeeded' AND error_code IS NULL)
        OR (status = 'failed' AND error_code IS NOT NULL))
);

CREATE INDEX idx_routing_raw_event_retention_run_finished
    ON routing_raw_event_retention_run(finished_at_ms DESC, run_id DESC);

CREATE INDEX idx_routing_observations_v3_retention
    ON routing_observations(ingestion_sequence, ingested_at_ms, event_at_ms)
    WHERE ingestion_sequence IS NOT NULL;

CREATE INDEX idx_routing_circuit_event_v3_retention
    ON routing_circuit_event_v3(ingestion_sequence, created_at_ms, occurred_at_ms)
    WHERE ingestion_sequence IS NOT NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 65,
    updated_by_migration = 65,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 65;

CREATE TEMP TABLE persistence_v65_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 65)
);
INSERT INTO persistence_v65_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v65_schema_guard;
