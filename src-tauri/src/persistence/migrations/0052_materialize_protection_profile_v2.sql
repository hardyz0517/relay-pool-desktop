-- Materialize the protection profile for V2 rows written before the profile
-- became a required field. This is an additive storage projection: existing
-- policy revisions and explicit profiles are preserved, while rows that are
-- already V2 but lack the nested profile receive the fail-closed default.

UPDATE routing_policy
SET config_json = json_set(
        config_json,
        '$.protectionProfile',
        json_object(
            'version', 1,
            'enabled', json('false'),
            'windowMaxSamples', 64,
            'windowMs', 300000,
            'minSamples', 5,
            'failureThresholdPercent', 60,
            'halfOpenSuccessesToClose', 2
        )
    )
WHERE singleton_key = 1
  AND json_extract(config_json, '$.version') = 2
  AND json_type(config_json, '$.protectionProfile') IS NULL;

UPDATE routing_policy_history
SET config_json = json_set(
        config_json,
        '$.protectionProfile',
        json_object(
            'version', 1,
            'enabled', json('false'),
            'windowMaxSamples', 64,
            'windowMs', 300000,
            'minSamples', 5,
            'failureThresholdPercent', 60,
            'halfOpenSuccessesToClose', 2
        )
    )
WHERE json_extract(config_json, '$.version') = 2
  AND json_type(config_json, '$.protectionProfile') IS NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 52,
    updated_by_migration = 52,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 52;

CREATE TEMP TABLE persistence_v52_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 52)
);
INSERT INTO persistence_v52_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v52_schema_guard;
