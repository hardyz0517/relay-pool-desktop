-- Intelligent routing cutover (schema 26).
--
-- This migration is intentionally destructive.  Routing policy and the
-- canonical observation/quality projections are the only active routing
-- inputs after the cutover.  Legacy settings and derived health strings are
-- discarded; historical request/monitor evidence remains in its own tables.

-- Legacy routing configuration is ambiguous and must never seed a V1 policy.
-- The policy aggregate already has an explicit baseline (0024/0025), so these
-- keys are safe to remove.  Import code classifies the same keys as Reset.
DELETE FROM settings WHERE key IN (
    'default_routing_strategy', 'default_routing_group_filter',
    'scheduler_advanced_settings_json', 'max_rate_multiplier',
    'allow_depleted_fallback'
);

ALTER TABLE channel_monitors RENAME COLUMN health_writeback_mode TO health_policy_mode;

-- The physical column/table removal is deliberately gated behind the source
-- zero-reference check (`scripts/routing-cutover-schema.test.mjs`).  Keeping
-- this migration additive until that gate is green prevents an installed
-- database from being upgraded into a schema the running binary cannot read.
-- The canonical policy/settings reset below is the safe part of the cutover;
-- the destructive DDL is released in the same atomic change as its readers.
-- Station/catalog status remains a user-facing asset state and is not a
-- router input. Its removal is deliberately outside this routing cutover.
-- Legacy health tables remain as non-routing compatibility storage during the
-- staged migration; routing never reads them for planning decisions.
ALTER TABLE channel_monitor_target_results DROP COLUMN health_writeback_mode;
ALTER TABLE channel_monitor_target_results DROP COLUMN health_writeback_decision;
ALTER TABLE channel_monitor_target_results DROP COLUMN health_writeback_reason;

-- Replace the pre-v2 derived health owners with canonical projection-owned
-- snapshots. The old tables are intentionally dropped; their durable evidence
-- remains in station_key_health_observations/routing_observations and is
-- rebuilt by the supervised projection runner.
DROP TABLE IF EXISTS station_endpoint_health;
DROP TABLE IF EXISTS station_key_health;

CREATE TABLE IF NOT EXISTS endpoint_health_snapshot (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'unchecked',
    latency_ms INTEGER,
    checked_at TEXT,
    error_summary TEXT,
    updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS routing_health_snapshot (
    station_key_id TEXT PRIMARY KEY REFERENCES station_keys(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL DEFAULT 1,
    last_success_at TEXT,
    last_failure_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    avg_latency_ms INTEGER,
    last_error_summary TEXT,
    cooldown_until TEXT,
    updated_at TEXT NOT NULL DEFAULT ''
);

UPDATE persistence_schema_compatibility
SET schema_version = 26,
    updated_by_migration = 26,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 26;
