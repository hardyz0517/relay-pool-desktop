ALTER TABLE remote_station_keys
ADD COLUMN discovery_order INTEGER NOT NULL DEFAULT 0 CHECK (discovery_order >= 0);

WITH ranked_remote_keys AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY station_id
            ORDER BY collected_at DESC, id ASC
        ) - 1 AS discovery_order
    FROM remote_station_keys
)
UPDATE remote_station_keys
SET discovery_order = (
    SELECT ranked.discovery_order
    FROM ranked_remote_keys ranked
    WHERE ranked.id = remote_station_keys.id
);

CREATE INDEX idx_remote_station_keys_discovery_order
    ON remote_station_keys(station_id, discovery_order, id);

UPDATE persistence_schema_compatibility
SET schema_version = 13,
    updated_by_migration = 13,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 12;

CREATE TEMP TABLE persistence_v13_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 13)
);

INSERT INTO persistence_v13_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v13_schema_guard;
