-- Allow each channel monitor to inherit the global network proxy or opt out.
ALTER TABLE channel_monitors ADD COLUMN proxy_mode TEXT NOT NULL DEFAULT 'inherit'
    CHECK (proxy_mode IN ('inherit', 'direct', 'system', 'manual'));
ALTER TABLE channel_monitors ADD COLUMN proxy_url TEXT
    CHECK (proxy_url IS NULL OR trim(proxy_url) <> '');

UPDATE persistence_schema_compatibility
SET schema_version = 58,
    updated_by_migration = 58,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 58;

CREATE TEMP TABLE persistence_v58_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 58)
);
INSERT INTO persistence_v58_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v58_schema_guard;
