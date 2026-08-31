-- Distinguish reducer transitions from duplicate/late audit-only events.
-- Generation rebuilds must replay only applied rows.
ALTER TABLE routing_circuit_event_v3
ADD COLUMN applied INTEGER NOT NULL DEFAULT 1 CHECK (applied IN (0, 1));

CREATE INDEX idx_routing_circuit_event_v3_applied_sequence
    ON routing_circuit_event_v3(
        station_key_id, station_key_lifecycle_revision,
        reducer_commit_sequence, ingestion_sequence
    )
    WHERE applied = 1;

UPDATE persistence_schema_compatibility
SET schema_version = 70,
    updated_by_migration = 70,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 70;

CREATE TEMP TABLE persistence_v70_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 70)
);
INSERT INTO persistence_v70_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v70_schema_guard;
