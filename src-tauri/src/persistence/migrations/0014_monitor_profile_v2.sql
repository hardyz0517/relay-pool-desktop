-- Built-in CLI compatibility Profiles are versioned request contracts. Upgrade
-- active definitions without rewriting historical attempts or target results.
UPDATE channel_monitors
SET client_profile_version = 2,
    schedule_revision = schedule_revision + 1,
    updated_at = strftime('%s', 'now') || '000'
WHERE client_profile_version = 1
  AND client_profile_id IN (
      'codex_cli_compat',
      'claude_code_compat',
      'gemini_cli_compat'
  );

UPDATE persistence_schema_compatibility
SET schema_version = 14,
    updated_by_migration = 14,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 13;

CREATE TEMP TABLE persistence_v14_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 14)
);

INSERT INTO persistence_v14_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v14_schema_guard;
