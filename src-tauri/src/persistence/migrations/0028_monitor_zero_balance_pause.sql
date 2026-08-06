ALTER TABLE channel_monitors ADD COLUMN pause_on_zero_balance INTEGER NOT NULL DEFAULT 1
    CHECK (pause_on_zero_balance IN (0, 1));

CREATE INDEX idx_balance_snapshots_latest_key_scope
    ON balance_snapshots(station_key_id, scope, updated_at DESC, created_at DESC, id DESC)
    WHERE station_key_id IS NOT NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 28,
    updated_by_migration = 28,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 27;

CREATE TEMP TABLE persistence_v28_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 28)
);

INSERT INTO persistence_v28_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v28_schema_guard;
