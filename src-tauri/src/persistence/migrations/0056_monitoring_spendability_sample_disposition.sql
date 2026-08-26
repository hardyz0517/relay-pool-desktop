-- Persist explicit spendability evidence and terminal sample disposition.
-- Existing rows intentionally remain unknown/legacy; no historical status is
-- promoted to authoritative depleted evidence by this migration.

ALTER TABLE balance_snapshots ADD COLUMN evidence_confidence TEXT NOT NULL DEFAULT 'unknown'
    CHECK (evidence_confidence IN ('confirmed', 'probable', 'unknown', 'conflicting'));
ALTER TABLE balance_snapshots ADD COLUMN spendability_authority TEXT NOT NULL DEFAULT 'unknown'
    CHECK (spendability_authority IN ('authoritative', 'advisory', 'unknown'));
ALTER TABLE balance_snapshots ADD COLUMN observed_at_ms INTEGER;
ALTER TABLE balance_snapshots ADD COLUMN valid_until_ms INTEGER;
ALTER TABLE balance_snapshots ADD COLUMN evidence_profile_version TEXT;
ALTER TABLE balance_snapshots ADD COLUMN spendability_reason_code TEXT;

ALTER TABLE channel_monitor_attempts ADD COLUMN canonical_failure_class TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN failure_origin TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN failure_scope_kind TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN failure_dimension TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN evidence_code TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN evidence_confidence TEXT;
ALTER TABLE channel_monitor_attempts ADD COLUMN classifier_profile_version TEXT;

ALTER TABLE channel_monitor_target_results ADD COLUMN availability_eligible INTEGER NOT NULL DEFAULT 1
    CHECK (availability_eligible IN (0, 1));
ALTER TABLE channel_monitor_target_results ADD COLUMN latency_eligible INTEGER NOT NULL DEFAULT 1
    CHECK (latency_eligible IN (0, 1));
ALTER TABLE channel_monitor_target_results ADD COLUMN exclusion_reason TEXT;
ALTER TABLE channel_monitor_target_results ADD COLUMN technical_health_effect TEXT NOT NULL DEFAULT 'legacy'
    CHECK (technical_health_effect IN ('positive', 'negative', 'neutral', 'legacy'));
ALTER TABLE channel_monitor_target_results ADD COLUMN disposition_profile_version TEXT NOT NULL DEFAULT 'legacy-monitoring-v1';

ALTER TABLE channel_monitor_bucket_rollups ADD COLUMN excluded_count INTEGER NOT NULL DEFAULT 0
    CHECK (excluded_count >= 0);
ALTER TABLE channel_monitor_bucket_rollups ADD COLUMN exclusion_counts_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(exclusion_counts_json) AND json_type(exclusion_counts_json) = 'object');

UPDATE persistence_schema_compatibility
SET schema_version = 56,
    updated_by_migration = 56,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 56;

CREATE TEMP TABLE persistence_v56_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 56)
);
INSERT INTO persistence_v56_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v56_schema_guard;
