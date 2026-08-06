-- Make the canonical quality projector replay from a monotonic receive clock.
-- Existing observations retain their event time in event_at_ms; the append
-- path now assigns ingested_at_ms from the serialized write path, clamped above
-- the previous maximum so late event timestamps cannot move the cursor back.
-- The old projection revision was based on producer-local sequences (including
-- random monitoring hashes and process-local request counters), so all derived
-- rows must be rebuilt under the v2 projector revision.
DELETE FROM routing_projector_checkpoints
WHERE projector = 'routing-projection-v1';

DELETE FROM routing_quality_summaries;
DELETE FROM routing_health_axes;

CREATE INDEX IF NOT EXISTS idx_routing_observations_ingestion_order
    ON routing_observations(ingested_at_ms ASC, id ASC);

UPDATE persistence_schema_compatibility
SET schema_version = 27,
    updated_by_migration = 27,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 27;
