-- Informational audit changes have attention state independent from incident
-- episodes.  Reading an informational change must not mutate alert lifecycle.
ALTER TABLE change_event_occurrences
ADD COLUMN seen_at_ms INTEGER CHECK (seen_at_ms IS NULL OR seen_at_ms >= 0);

CREATE INDEX idx_change_event_occurrences_audit_unseen_observed
    ON change_event_occurrences(seen_at_ms, observed_at_ms DESC, id DESC)
    WHERE incident_id IS NULL AND category = 'audit_change';

UPDATE persistence_schema_compatibility
SET schema_version = 40,
    updated_by_migration = 40,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 39;

CREATE TEMP TABLE persistence_v40_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 40)
);

INSERT INTO persistence_v40_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v40_schema_guard;
