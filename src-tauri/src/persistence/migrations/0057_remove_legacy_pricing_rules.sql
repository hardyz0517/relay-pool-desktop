-- The former pricing_rules subsystem mixed station-local fixed prices,
-- per-model overrides, and multiplier fallbacks.  None of those values can
-- be translated losslessly into the model-base-price + group-multiplier
-- model, so the upgrade relies on the pre-upgrade backup and removes them.

DROP INDEX IF EXISTS idx_change_event_occurrences_incident_episode_observed;
DROP INDEX IF EXISTS idx_change_event_occurrences_type_observed;
DROP INDEX IF EXISTS idx_change_event_occurrences_audit_unseen_observed;

ALTER TABLE change_event_occurrences RENAME TO change_event_occurrences_v56;

CREATE TABLE change_event_occurrences (
    id TEXT PRIMARY KEY,
    source_observation_key TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('audit_change', 'condition_observation')),
    observation_kind TEXT NOT NULL CHECK (observation_kind IN ('abnormal', 'healthy', 'change')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'critical')),
    condition_key TEXT,
    incident_id TEXT REFERENCES change_incidents(id) ON DELETE SET NULL,
    episode_number INTEGER CHECK (episode_number IS NULL OR episode_number > 0),
    object_type TEXT NOT NULL,
    object_id TEXT,
    station_id TEXT REFERENCES stations(id) ON DELETE SET NULL,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    request_log_id TEXT REFERENCES request_logs(id) ON DELETE SET NULL,
    source TEXT NOT NULL,
    reason_code TEXT,
    old_value_json TEXT CHECK (old_value_json IS NULL OR json_valid(old_value_json)),
    new_value_json TEXT CHECK (new_value_json IS NULL OR json_valid(new_value_json)),
    impact_json TEXT CHECK (impact_json IS NULL OR json_valid(impact_json)),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    seen_at_ms INTEGER CHECK (seen_at_ms IS NULL OR seen_at_ms >= 0)
);

INSERT INTO change_event_occurrences (
    id, source_observation_key, event_type, category, observation_kind,
    severity, condition_key, incident_id, episode_number, object_type,
    object_id, station_id, station_key_id, request_log_id, source, reason_code,
    old_value_json, new_value_json, impact_json, observed_at_ms, created_at_ms,
    seen_at_ms
)
SELECT
    id, source_observation_key, event_type, category, observation_kind,
    severity, condition_key, incident_id, episode_number, object_type,
    object_id, station_id, station_key_id, request_log_id, source, reason_code,
    old_value_json, new_value_json, impact_json, observed_at_ms, created_at_ms,
    seen_at_ms
FROM change_event_occurrences_v56;

DROP TABLE change_event_occurrences_v56;

CREATE INDEX idx_change_event_occurrences_incident_episode_observed
    ON change_event_occurrences(incident_id, episode_number, observed_at_ms DESC, id DESC);
CREATE INDEX idx_change_event_occurrences_type_observed
    ON change_event_occurrences(event_type, observed_at_ms DESC, id DESC);
CREATE INDEX idx_change_event_occurrences_audit_unseen_observed
    ON change_event_occurrences(seen_at_ms, observed_at_ms DESC, id DESC)
    WHERE incident_id IS NULL AND category = 'audit_change';

ALTER TABLE request_logs DROP COLUMN base_fixed_cost;
ALTER TABLE request_logs DROP COLUMN base_input_cost;
ALTER TABLE request_logs DROP COLUMN base_output_cost;
ALTER TABLE request_logs DROP COLUMN base_total_cost;
ALTER TABLE request_logs DROP COLUMN pricing_rule_id;
DROP TABLE pricing_rules;

UPDATE persistence_schema_compatibility
SET schema_version = 57,
    updated_by_migration = 57,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 57;

CREATE TEMP TABLE persistence_v57_pricing_rules_guard (
    legacy_table_count INTEGER NOT NULL CHECK (legacy_table_count = 0),
    legacy_column_count INTEGER NOT NULL CHECK (legacy_column_count = 0),
    legacy_reference_count INTEGER NOT NULL CHECK (legacy_reference_count = 0),
    foreign_key_violation_count INTEGER NOT NULL CHECK (foreign_key_violation_count = 0),
    schema_version INTEGER NOT NULL CHECK (schema_version = 57)
);

INSERT INTO persistence_v57_pricing_rules_guard (
    legacy_table_count,
    legacy_column_count,
    legacy_reference_count,
    foreign_key_violation_count,
    schema_version
)
SELECT
    (SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'pricing_rules'),
    (SELECT COUNT(*) FROM pragma_table_info('request_logs')
     WHERE name IN (
         'base_fixed_cost', 'base_input_cost', 'base_output_cost', 'base_total_cost',
         'pricing_rule_id'
     ))
     + (SELECT COUNT(*) FROM pragma_table_info('change_event_occurrences')
         WHERE name = 'pricing_rule_id'),
    (SELECT COUNT(*) FROM sqlite_master
     WHERE sql IS NOT NULL AND lower(sql) LIKE '%pricing_rules%'),
    (SELECT COUNT(*) FROM pragma_foreign_key_check),
    schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v57_pricing_rules_guard;
