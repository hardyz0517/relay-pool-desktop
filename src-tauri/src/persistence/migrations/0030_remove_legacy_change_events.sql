-- Destructive legacy cleanup. Startup upgrade deliberately applies this
-- migration only after the durable alerting backfill and current-facts rebuild.
-- Fresh databases may have an empty legacy table; the operation is idempotent.

DROP INDEX IF EXISTS idx_change_events_status_severity_updated;
DROP INDEX IF EXISTS idx_change_events_station_updated;
DROP INDEX IF EXISTS idx_change_events_station_key_updated;
DROP INDEX IF EXISTS idx_change_events_page;
DROP INDEX IF EXISTS idx_change_events_station_page;
DROP TABLE IF EXISTS change_events;

UPDATE persistence_schema_compatibility
SET schema_version = 30,
    updated_by_migration = 30,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 29;

CREATE TEMP TABLE persistence_v30_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 30)
);

INSERT INTO persistence_v30_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v30_schema_guard;
