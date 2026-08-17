-- A station-level missing group is an append-only informational change, not
-- an alert lifecycle. Convert older incident projections without retaining a
-- synthetic recovery record or a scheduled/delivered alert trail.

INSERT OR IGNORE INTO change_event_occurrences (
    id, source_observation_key, event_type, category, observation_kind, severity,
    condition_key, object_type, object_id, station_id, station_key_id, source,
    reason_code, new_value_json, observed_at_ms, created_at_ms, seen_at_ms
)
SELECT
    'migration-group-missing-' || i.id,
    'migration:group_missing:' || i.id,
    'group_missing', 'audit_change', 'change', 'info',
    i.condition_key, i.object_type, i.object_id, i.station_id, i.station_key_id,
    'migration', 'group_missing', i.last_observation_summary_json,
    i.updated_at_ms, i.created_at_ms,
    (
        SELECT a.seen_at_ms
        FROM incident_attention a
        WHERE a.incident_id = i.id AND a.episode_number = i.episode_number
    )
FROM change_incidents i
WHERE i.event_type = 'group_missing'
  AND NOT EXISTS (
      SELECT 1
      FROM change_event_occurrences o
      WHERE o.incident_id = i.id AND o.observation_kind = 'abnormal'
  );

UPDATE change_event_occurrences
SET seen_at_ms = COALESCE(
    seen_at_ms,
    (
        SELECT a.seen_at_ms
        FROM incident_attention a
        WHERE a.incident_id = change_event_occurrences.incident_id
          AND a.episode_number = change_event_occurrences.episode_number
    )
)
WHERE observation_kind = 'abnormal'
  AND incident_id IN (
      SELECT id FROM change_incidents WHERE event_type = 'group_missing'
  );

UPDATE change_event_occurrences
SET category = 'audit_change',
    observation_kind = 'change',
    severity = 'info',
    incident_id = NULL,
    episode_number = NULL
WHERE observation_kind = 'abnormal'
  AND incident_id IN (
      SELECT id FROM change_incidents WHERE event_type = 'group_missing'
  );

-- Healthy observations were formerly used solely to resolve the incident.
-- Do not retain them as a second, recovery-style information record.
DELETE FROM change_event_occurrences
WHERE incident_id IN (
    SELECT id FROM change_incidents WHERE event_type = 'group_missing'
);

DELETE FROM change_incidents WHERE event_type = 'group_missing';

UPDATE persistence_schema_compatibility
SET schema_version = 42,
    updated_by_migration = 42,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 41;

CREATE TEMP TABLE persistence_v42_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 42)
);

INSERT INTO persistence_v42_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v42_schema_guard;
