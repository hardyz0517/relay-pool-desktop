-- Group-rate projection used to advance station-key lifecycle revisions even
-- though it changed only derived group/rate attributes. Keep immutable v3
-- facts untouched and record a quality-only compatibility alias for rows that
-- can be identified as belonging to that projection.
CREATE TABLE routing_quality_lifecycle_alias_v1 (
    station_key_id TEXT PRIMARY KEY CHECK (length(station_key_id) BETWEEN 1 AND 160),
    target_lifecycle_revision INTEGER NOT NULL CHECK (target_lifecycle_revision > 0),
    reason_code TEXT NOT NULL CHECK (reason_code = 'group_rate_projection_lifecycle_drift'),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TEMP TABLE routing_v3_lifecycle_repair_keys (
    station_key_id TEXT PRIMARY KEY,
    lifecycle_revision INTEGER NOT NULL CHECK (lifecycle_revision > 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

INSERT INTO routing_v3_lifecycle_repair_keys (station_key_id, lifecycle_revision, created_at_ms)
SELECT k.id, r.revision, r.updated_at_ms
FROM station_keys k
JOIN domain_revisions r ON r.scope = 'station_key:' || k.id
WHERE k.rate_source = 'sub2api_groups_rates'
  AND k.rate_collected_at IS NOT NULL
  AND trim(k.rate_collected_at) <> ''
  AND trim(k.rate_collected_at) NOT GLOB '*[^0-9]*'
  AND CAST(k.rate_collected_at AS INTEGER) = r.updated_at_ms
  AND r.revision > 1
  AND EXISTS (
      SELECT 1
      FROM routing_observations o
      WHERE o.station_key_id = k.id
        AND o.station_key_lifecycle_revision IS NOT NULL
  )
  AND NOT EXISTS (
      SELECT 1
      FROM routing_observations o
      WHERE o.station_key_id = k.id
      GROUP BY o.source, o.correlation_id, o.attempt_index
      HAVING COUNT(DISTINCT o.station_key_lifecycle_revision) > 1
  )
  AND NOT EXISTS (
      SELECT 1
      FROM routing_attempt_v3 a
      WHERE a.station_key_id = k.id
      GROUP BY a.source, a.correlation_id, a.attempt_index
      HAVING COUNT(DISTINCT a.station_key_lifecycle_revision) > 1
  )
  AND NOT EXISTS (
      SELECT 1
      FROM routing_attempt_cluster_v3 c
      WHERE c.station_key_id = k.id
      GROUP BY c.source, c.correlation_id
      HAVING COUNT(DISTINCT c.station_key_lifecycle_revision) > 1
  );

INSERT INTO routing_quality_lifecycle_alias_v1 (
    station_key_id, target_lifecycle_revision, reason_code, created_at_ms
)
SELECT station_key_id, lifecycle_revision,
       'group_rate_projection_lifecycle_drift', created_at_ms
FROM routing_v3_lifecycle_repair_keys;

DROP TABLE routing_v3_lifecycle_repair_keys;

UPDATE persistence_schema_compatibility
SET schema_version = 71,
    updated_by_migration = 71,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 71;

CREATE TEMP TABLE persistence_v71_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 71)
);
INSERT INTO persistence_v71_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v71_schema_guard;
