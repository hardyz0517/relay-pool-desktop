-- Align Key Pool monitor defaults with Sub2API channel monitor behavior:
-- a single provider request may run for 45s, with an execution budget that
-- leaves room for orchestration overhead.
UPDATE channel_monitors
SET attempt_timeout_ms = 45000,
    execution_timeout_ms = 60000,
    schedule_revision = schedule_revision + 1,
    updated_at = strftime('%s', 'now') || '000'
WHERE note = '由密钥池监控开关创建'
  AND attempt_timeout_ms = 30000
  AND execution_timeout_ms = 45000;

UPDATE persistence_schema_compatibility
SET schema_version = 16,
    updated_by_migration = 16,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 15;

CREATE TEMP TABLE persistence_v16_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 16)
);

INSERT INTO persistence_v16_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v16_schema_guard;
