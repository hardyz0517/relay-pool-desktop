-- Add provider-reported success-rate metrics after the initial NewAPI
-- performance migration. Kept separate so databases that already applied
-- migration 78 can be upgraded without checksum drift or data loss.
ALTER TABLE station_published_monitors ADD COLUMN current_success_rate_percent REAL
    CHECK (current_success_rate_percent IS NULL OR (current_success_rate_percent >= 0 AND current_success_rate_percent <= 100));

ALTER TABLE station_published_monitor_samples ADD COLUMN success_rate_percent REAL
    CHECK (success_rate_percent IS NULL OR (success_rate_percent >= 0 AND success_rate_percent <= 100));

UPDATE persistence_schema_compatibility
SET schema_version = 79,
    updated_by_migration = 79,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version < 79;

CREATE TEMP TABLE persistence_v79_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 79)
);

INSERT INTO persistence_v79_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v79_schema_guard;
