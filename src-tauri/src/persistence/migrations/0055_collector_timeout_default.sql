-- Upgrade rows that still equal the historical collector-timeout default.
-- Other values are preserved; the setting has no provenance with which to
-- distinguish an intentional custom value of 15 from the historical default.

UPDATE settings
SET value = '30',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE key = 'collector_timeout_seconds'
  AND trim(value) = '15';

UPDATE persistence_schema_compatibility
SET schema_version = 55,
    updated_by_migration = 55,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 55;

CREATE TEMP TABLE persistence_v55_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 55)
);
INSERT INTO persistence_v55_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v55_schema_guard;

CREATE TEMP TABLE persistence_v55_timeout_guard (
    historical_default_count INTEGER NOT NULL CHECK (historical_default_count = 0)
);
INSERT INTO persistence_v55_timeout_guard (historical_default_count)
SELECT COUNT(*)
FROM settings
WHERE key = 'collector_timeout_seconds'
  AND trim(value) = '15';
DROP TABLE persistence_v55_timeout_guard;
