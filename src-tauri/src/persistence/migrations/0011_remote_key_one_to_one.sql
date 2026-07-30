-- A routeable Station Key represents one concrete upstream credential. Preserve
-- the strongest deterministic owner before enforcing one-to-one matching.
WITH ranked_matches AS (
    SELECT
        remote.id,
        ROW_NUMBER() OVER (
            PARTITION BY remote.matched_station_key_id
            ORDER BY
                CASE
                    WHEN local.note = '由远端发现开关自动创建：' || remote.id THEN 0
                    ELSE 1
                END,
                remote.match_confidence DESC,
                remote.collected_at DESC,
                remote.id ASC
        ) AS owner_rank
    FROM remote_station_keys remote
    JOIN station_keys local ON local.id = remote.matched_station_key_id
    WHERE remote.matched_station_key_id IS NOT NULL
)
UPDATE remote_station_keys
SET match_status = 'unbound',
    matched_station_key_id = NULL,
    match_confidence = 0.0,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id IN (
    SELECT id
    FROM ranked_matches
    WHERE owner_rank > 1
);

CREATE UNIQUE INDEX idx_remote_station_keys_one_local_owner
    ON remote_station_keys(matched_station_key_id)
    WHERE matched_station_key_id IS NOT NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 11,
    updated_by_migration = 11,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 10;

CREATE TEMP TABLE persistence_v11_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 11)
);

INSERT INTO persistence_v11_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v11_schema_guard;
