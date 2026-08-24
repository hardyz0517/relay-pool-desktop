-- Materialize the user-editable proxy timeout profile for V2 policy rows.
-- Existing values keep the historical proxy defaults; this migration only
-- adds the missing nested object to rows written before timeout editing.

UPDATE routing_policy
SET config_json = json_set(
        config_json,
        '$.timeoutPolicy',
        json_object(
            'version', 1,
            'connectMs', 10000,
            'firstByteMs', 30000,
            'precommitMs', 60000,
            'bufferedExecutionMs', 300000,
            'streamIdleMs', 90000
        )
    )
WHERE singleton_key = 1
  AND json_extract(config_json, '$.version') = 2
  AND json_type(config_json, '$.timeoutPolicy') IS NULL;

UPDATE routing_policy_history
SET config_json = json_set(
        config_json,
        '$.timeoutPolicy',
        json_object(
            'version', 1,
            'connectMs', 10000,
            'firstByteMs', 30000,
            'precommitMs', 60000,
            'bufferedExecutionMs', 300000,
            'streamIdleMs', 90000
        )
    )
WHERE json_extract(config_json, '$.version') = 2
  AND json_type(config_json, '$.timeoutPolicy') IS NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 53,
    updated_by_migration = 53,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 53;

CREATE TEMP TABLE persistence_v53_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 53)
);
INSERT INTO persistence_v53_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v53_schema_guard;
