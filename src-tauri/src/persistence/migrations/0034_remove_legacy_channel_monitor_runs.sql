DROP TABLE IF EXISTS channel_monitor_runs;

UPDATE persistence_schema_compatibility
SET schema_version = 34,
    updated_by_migration = 34,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 33;

CREATE TEMP TABLE persistence_v34_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 34)
);

INSERT INTO persistence_v34_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v34_schema_guard;
