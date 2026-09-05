ALTER TABLE request_logs
ADD COLUMN resolved_upstream_model TEXT;

ALTER TABLE request_attempts
ADD COLUMN resolved_upstream_model TEXT;

UPDATE persistence_schema_compatibility
SET schema_version = 75, updated_by_migration = 75,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version < 75;

CREATE TEMP TABLE persistence_v75_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 75)
);
INSERT INTO persistence_v75_schema_guard (schema_version)
SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1;
DROP TABLE persistence_v75_schema_guard;
