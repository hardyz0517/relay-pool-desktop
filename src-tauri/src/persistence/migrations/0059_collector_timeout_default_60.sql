-- Upgrade rows that still equal the previous collector-timeout default.
-- Other values are preserved; the setting has no provenance with which to
-- distinguish an intentional custom value of 30 from the historical default.

UPDATE settings
SET value = '60',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE key = 'collector_timeout_seconds'
  AND trim(value) = '30';

UPDATE persistence_schema_compatibility
SET schema_version = 59,
    updated_by_migration = 59,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 59;

CREATE TEMP TABLE persistence_v59_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 59)
);
INSERT INTO persistence_v59_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v59_schema_guard;

CREATE TEMP TABLE persistence_v59_timeout_guard (
    previous_default_count INTEGER NOT NULL CHECK (previous_default_count = 0)
);
INSERT INTO persistence_v59_timeout_guard (previous_default_count)
SELECT COUNT(*)
FROM settings
WHERE key = 'collector_timeout_seconds'
  AND trim(value) = '30';
DROP TABLE persistence_v59_timeout_guard;
