ALTER TABLE station_credentials ADD COLUMN login_username TEXT;
ALTER TABLE station_credentials ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

ALTER TABLE station_endpoint_health ADD COLUMN status TEXT NOT NULL DEFAULT 'unchecked';
ALTER TABLE station_endpoint_health ADD COLUMN latency_ms INTEGER;
ALTER TABLE station_endpoint_health ADD COLUMN checked_at TEXT;
ALTER TABLE station_endpoint_health ADD COLUMN error_summary TEXT;
ALTER TABLE station_endpoint_health ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE persistence_schema_compatibility
SET schema_version = 8,
    updated_by_migration = 8,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 7;
