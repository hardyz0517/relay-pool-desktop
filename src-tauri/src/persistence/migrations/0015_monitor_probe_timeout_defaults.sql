-- Key Pool monitors created with the old defaults timed out before slow but
-- otherwise healthy providers emitted their first token. Preserve customized
-- monitor budgets and only upgrade the exact legacy Key Pool defaults.
UPDATE channel_monitors
SET attempt_timeout_ms = 30000,
    execution_timeout_ms = 45000,
    schedule_revision = schedule_revision + 1,
    updated_at = strftime('%s', 'now') || '000'
WHERE note = '由密钥池监控开关创建'
  AND attempt_timeout_ms = 10000
  AND execution_timeout_ms = 30000;

UPDATE persistence_schema_compatibility
SET schema_version = 15,
    updated_by_migration = 15,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 14;

CREATE TEMP TABLE persistence_v15_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 15)
);

INSERT INTO persistence_v15_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v15_schema_guard;
