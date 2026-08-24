-- Durable metadata for the unified scoped health state machine. Immutable
-- routing_health_observations and the scoped verdict generation remain the
-- only health evidence/projection owners; this bounded row stores reducer
-- state (revision, cooldown and idempotency window) for recovery.
CREATE TABLE routing_health_protection_state (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    profile_version TEXT NOT NULL CHECK (length(profile_version) BETWEEN 1 AND 96),
    profile_json TEXT NOT NULL CHECK (length(profile_json) BETWEEN 2 AND 262144),
    snapshot_version TEXT NOT NULL CHECK (length(snapshot_version) BETWEEN 1 AND 96),
    snapshot_json TEXT NOT NULL CHECK (length(snapshot_json) BETWEEN 2 AND 4194304),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    generated_at_ms INTEGER NOT NULL CHECK (generated_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

UPDATE persistence_schema_compatibility
SET schema_version = 48,
    updated_by_migration = 48,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version = 47;

CREATE TEMP TABLE persistence_v48_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 48)
);
INSERT INTO persistence_v48_schema_guard (schema_version)
SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1;
DROP TABLE persistence_v48_schema_guard;
