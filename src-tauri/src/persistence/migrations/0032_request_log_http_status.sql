ALTER TABLE request_logs
ADD COLUMN http_status INTEGER
CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599);

UPDATE persistence_schema_compatibility
SET schema_version = 32,
    updated_by_migration = 32,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 31;

CREATE TEMP TABLE persistence_v32_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 32)
);

INSERT INTO persistence_v32_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v32_schema_guard;
