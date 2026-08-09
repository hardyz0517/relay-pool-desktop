ALTER TABLE station_credentials
ADD COLUMN session_user_agent TEXT;

UPDATE persistence_schema_compatibility
SET schema_version = 31,
    updated_by_migration = 31,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 30;

CREATE TEMP TABLE persistence_v31_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 31)
);

INSERT INTO persistence_v31_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v31_schema_guard;
