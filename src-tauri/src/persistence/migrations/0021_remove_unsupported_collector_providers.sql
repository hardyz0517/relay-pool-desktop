-- Remove unsupported station-collector providers while preserving user-owned
-- station, key, credential, request-log, and collector-run history.

UPDATE stations
SET enabled = 0,
    status = 'disabled',
    updated_at = strftime('%s', 'now')
WHERE station_type IN ('openai-compatible', 'openai_compatible', 'custom');

DELETE FROM collector_task_state
WHERE task_type = 'models'
   OR station_id IN (
        SELECT id
        FROM stations
        WHERE station_type IN ('openai-compatible', 'openai_compatible', 'custom')
   );

DELETE FROM collector_model_facts
WHERE station_id IN (
        SELECT id
        FROM stations
        WHERE station_type IN ('newapi', 'openai-compatible', 'openai_compatible', 'custom')
   );

DELETE FROM change_events
WHERE event_type IN ('model_added', 'model_removed')
  AND (
        station_id IN (
            SELECT id
            FROM stations
            WHERE station_type IN ('newapi', 'openai-compatible', 'openai_compatible', 'custom')
        )
        OR source LIKE 'collector%'
      );

DELETE FROM settings
WHERE key = 'model_list_interval_minutes';

UPDATE persistence_schema_compatibility
SET schema_version = 21,
    updated_by_migration = 21,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 21;
