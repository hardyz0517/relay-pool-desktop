-- Alerting foundation. History backfill and current-fact rebuild are durable
-- upgrade work owned by a later supervised upgrade step; this migration only
-- establishes the append-only storage contract.

CREATE TABLE alert_policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    state TEXT NOT NULL DEFAULT 'active'
        CHECK (state IN ('active', 'disabled', 'orphaned', 'tombstone')),
    scope_kind TEXT NOT NULL
        CHECK (scope_kind IN ('global', 'event_type', 'station', 'station_key')),
    event_type TEXT,
    -- Policy targets intentionally do not use a physical FK. Deleting a
    -- station/key must leave a durable orphaned policy for user review rather
    -- than fail the asset deletion or violate the scope CHECK constraint.
    station_id TEXT,
    station_key_id TEXT,
    minimum_severity TEXT CHECK (minimum_severity IS NULL OR minimum_severity IN ('info', 'warning', 'critical')),
    severity_offset INTEGER NOT NULL DEFAULT 0 CHECK (severity_offset BETWEEN -1 AND 1),
    trigger_mode TEXT NOT NULL
        CHECK (trigger_mode IN ('immediate', 'consecutive_occurrences', 'active_duration')),
    trigger_count INTEGER CHECK (trigger_count IS NULL OR trigger_count BETWEEN 1 AND 100),
    trigger_duration_seconds INTEGER
        CHECK (trigger_duration_seconds IS NULL OR trigger_duration_seconds BETWEEN 60 AND 2592000),
    recovery_mode TEXT NOT NULL
        CHECK (recovery_mode IN ('consecutive_healthy', 'healthy_duration')),
    recovery_count INTEGER CHECK (recovery_count IS NULL OR recovery_count BETWEEN 1 AND 100),
    recovery_duration_seconds INTEGER
        CHECK (recovery_duration_seconds IS NULL OR recovery_duration_seconds BETWEEN 60 AND 2592000),
    in_app_enabled INTEGER NOT NULL DEFAULT 1 CHECK (in_app_enabled IN (0, 1)),
    desktop_enabled INTEGER NOT NULL DEFAULT 0 CHECK (desktop_enabled IN (0, 1)),
    repeat_mode TEXT NOT NULL DEFAULT 'never'
        CHECK (repeat_mode IN ('never', 'interval', 'severity_escalation', 'interval_and_escalation')),
    repeat_interval_seconds INTEGER
        CHECK (repeat_interval_seconds IS NULL OR repeat_interval_seconds > 0),
    cooldown_seconds INTEGER NOT NULL DEFAULT 1800 CHECK (cooldown_seconds >= 0),
    recovery_notification_enabled INTEGER NOT NULL DEFAULT 1 CHECK (recovery_notification_enabled IN (0, 1)),
    quiet_hours_policy TEXT NOT NULL DEFAULT 'inherit'
        CHECK (quiet_hours_policy IN ('inherit', 'respect', 'bypass_for_critical')),
    priority INTEGER NOT NULL DEFAULT 100 CHECK (priority >= 0),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    CHECK (
        (scope_kind = 'global' AND station_id IS NULL AND station_key_id IS NULL)
        OR (scope_kind = 'event_type' AND event_type IS NOT NULL AND station_id IS NULL AND station_key_id IS NULL)
        OR (scope_kind = 'station' AND station_id IS NOT NULL AND station_key_id IS NULL)
        OR (scope_kind = 'station_key' AND station_key_id IS NOT NULL)
    ),
    CHECK (
        (trigger_mode = 'immediate' AND trigger_count IS NULL AND trigger_duration_seconds IS NULL)
        OR (trigger_mode = 'consecutive_occurrences' AND trigger_count IS NOT NULL AND trigger_duration_seconds IS NULL)
        OR (trigger_mode = 'active_duration' AND trigger_count IS NULL AND trigger_duration_seconds IS NOT NULL)
    ),
    CHECK (
        (recovery_mode = 'consecutive_healthy' AND recovery_count IS NOT NULL AND recovery_duration_seconds IS NULL)
        OR (recovery_mode = 'healthy_duration' AND recovery_count IS NULL AND recovery_duration_seconds IS NOT NULL)
    ),
    CHECK (
        (repeat_mode IN ('never', 'severity_escalation') AND repeat_interval_seconds IS NULL)
        OR (repeat_mode IN ('interval', 'interval_and_escalation') AND repeat_interval_seconds IS NOT NULL)
    )
);

CREATE TABLE change_incidents (
    id TEXT PRIMARY KEY,
    condition_key TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL
        CHECK (lifecycle_state IN ('pending', 'open', 'recovering', 'resolved')),
    base_severity TEXT NOT NULL CHECK (base_severity IN ('info', 'warning', 'critical')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'critical')),
    object_type TEXT NOT NULL,
    object_id TEXT,
    station_id TEXT REFERENCES stations(id) ON DELETE SET NULL,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    policy_id TEXT REFERENCES alert_policies(id) ON DELETE SET NULL,
    policy_revision INTEGER CHECK (policy_revision IS NULL OR policy_revision > 0),
    lifecycle_policy_fingerprint TEXT NOT NULL,
    episode_number INTEGER NOT NULL CHECK (episode_number > 0),
    first_seen_at_ms INTEGER NOT NULL CHECK (first_seen_at_ms >= 0),
    last_seen_at_ms INTEGER NOT NULL CHECK (last_seen_at_ms >= 0),
    opened_at_ms INTEGER CHECK (opened_at_ms IS NULL OR opened_at_ms >= 0),
    recovering_at_ms INTEGER CHECK (recovering_at_ms IS NULL OR recovering_at_ms >= 0),
    resolved_at_ms INTEGER CHECK (resolved_at_ms IS NULL OR resolved_at_ms >= 0),
    occurrence_count INTEGER NOT NULL DEFAULT 0 CHECK (occurrence_count >= 0),
    episode_occurrence_count INTEGER NOT NULL DEFAULT 0 CHECK (episode_occurrence_count >= 0),
    consecutive_abnormal_count INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_abnormal_count >= 0),
    consecutive_healthy_count INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_healthy_count >= 0),
    pending_since_ms INTEGER CHECK (pending_since_ms IS NULL OR pending_since_ms >= 0),
    healthy_since_ms INTEGER CHECK (healthy_since_ms IS NULL OR healthy_since_ms >= 0),
    last_observation_id TEXT,
    last_observation_summary_json TEXT NOT NULL CHECK (json_valid(last_observation_summary_json)),
    fact_fresh_until_ms INTEGER CHECK (fact_fresh_until_ms IS NULL OR fact_fresh_until_ms >= 0),
    next_state_evaluation_at_ms INTEGER CHECK (next_state_evaluation_at_ms IS NULL OR next_state_evaluation_at_ms >= 0),
    last_notification_at_ms INTEGER CHECK (last_notification_at_ms IS NULL OR last_notification_at_ms >= 0),
    next_notification_at_ms INTEGER CHECK (next_notification_at_ms IS NULL OR next_notification_at_ms >= 0),
    version INTEGER NOT NULL DEFAULT 0 CHECK (version >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

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
    pricing_rule_id TEXT REFERENCES pricing_rules(id) ON DELETE SET NULL,
    request_log_id TEXT REFERENCES request_logs(id) ON DELETE SET NULL,
    source TEXT NOT NULL,
    reason_code TEXT,
    old_value_json TEXT CHECK (old_value_json IS NULL OR json_valid(old_value_json)),
    new_value_json TEXT CHECK (new_value_json IS NULL OR json_valid(new_value_json)),
    impact_json TEXT CHECK (impact_json IS NULL OR json_valid(impact_json)),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TABLE incident_attention (
    incident_id TEXT NOT NULL REFERENCES change_incidents(id) ON DELETE CASCADE,
    episode_number INTEGER NOT NULL CHECK (episode_number > 0),
    seen_at_ms INTEGER CHECK (seen_at_ms IS NULL OR seen_at_ms >= 0),
    acknowledged_at_ms INTEGER CHECK (acknowledged_at_ms IS NULL OR acknowledged_at_ms >= 0),
    acknowledged_reason TEXT CHECK (acknowledged_reason IS NULL OR length(acknowledged_reason) <= 500),
    snoozed_until_ms INTEGER CHECK (snoozed_until_ms IS NULL OR snoozed_until_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (incident_id, episode_number)
);

CREATE TABLE notification_deliveries (
    id TEXT PRIMARY KEY,
    delivery_key TEXT NOT NULL UNIQUE,
    incident_id TEXT NOT NULL REFERENCES change_incidents(id) ON DELETE CASCADE,
    episode_number INTEGER NOT NULL CHECK (episode_number > 0),
    delivery_sequence INTEGER NOT NULL CHECK (delivery_sequence > 0),
    policy_id TEXT REFERENCES alert_policies(id) ON DELETE SET NULL,
    policy_revision INTEGER CHECK (policy_revision IS NULL OR policy_revision > 0),
    policy_snapshot_json TEXT NOT NULL CHECK (json_valid(policy_snapshot_json)),
    channel TEXT NOT NULL CHECK (channel IN ('in_app', 'desktop')),
    delivery_kind TEXT NOT NULL CHECK (delivery_kind IN ('opened', 'repeated', 'escalated', 'recovered', 'test')),
    status TEXT NOT NULL
        CHECK (status IN ('scheduled', 'claimed', 'delivered', 'suppressed', 'failed', 'outcome_unknown')),
    scheduled_at_ms INTEGER NOT NULL CHECK (scheduled_at_ms >= 0),
    claim_token TEXT,
    claimed_at_ms INTEGER CHECK (claimed_at_ms IS NULL OR claimed_at_ms >= 0),
    lease_expires_at_ms INTEGER CHECK (lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    attempted_at_ms INTEGER CHECK (attempted_at_ms IS NULL OR attempted_at_ms >= 0),
    outcome_unknown_at_ms INTEGER CHECK (outcome_unknown_at_ms IS NULL OR outcome_unknown_at_ms >= 0),
    retry_not_before_ms INTEGER CHECK (retry_not_before_ms IS NULL OR retry_not_before_ms >= 0),
    delivered_at_ms INTEGER CHECK (delivered_at_ms IS NULL OR delivered_at_ms >= 0),
    suppressed_reason TEXT CHECK (suppressed_reason IS NULL OR suppressed_reason IN (
        'global_disabled', 'channel_disabled', 'permission_denied', 'quiet_hours',
        'global_pause', 'incident_snoozed', 'cooldown', 'repeat_disabled',
        'policy_muted', 'stale_episode'
    )),
    error_code TEXT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    UNIQUE (incident_id, episode_number, channel, delivery_kind, delivery_sequence)
);

CREATE TABLE alerting_upgrade_progress (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    phase TEXT NOT NULL
        CHECK (phase IN ('not_started', 'copying_history', 'rebuilding_current', 'verifying', 'complete', 'failed')),
    source_high_water_cursor TEXT,
    last_copied_cursor TEXT,
    copied_count INTEGER NOT NULL DEFAULT 0 CHECK (copied_count >= 0),
    rebuild_version INTEGER,
    last_error_code TEXT,
    started_at_ms INTEGER CHECK (started_at_ms IS NULL OR started_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    completed_at_ms INTEGER CHECK (completed_at_ms IS NULL OR completed_at_ms >= 0)
);

INSERT INTO alerting_upgrade_progress (singleton_key, phase, updated_at_ms)
VALUES (1, 'not_started', 0);

CREATE INDEX idx_change_incidents_lifecycle_severity_updated
    ON change_incidents(lifecycle_state, severity, updated_at_ms DESC, id DESC);
CREATE INDEX idx_change_incidents_station_lifecycle_updated
    ON change_incidents(station_id, lifecycle_state, updated_at_ms DESC);
CREATE INDEX idx_change_incidents_station_key_lifecycle_updated
    ON change_incidents(station_key_id, lifecycle_state, updated_at_ms DESC);
CREATE INDEX idx_change_event_occurrences_incident_episode_observed
    ON change_event_occurrences(incident_id, episode_number, observed_at_ms DESC, id DESC);
CREATE INDEX idx_change_event_occurrences_type_observed
    ON change_event_occurrences(event_type, observed_at_ms DESC, id DESC);
CREATE INDEX idx_alert_policies_enabled_scope_priority
    ON alert_policies(enabled, scope_kind, priority, id);
CREATE INDEX idx_notification_deliveries_status_scheduled
    ON notification_deliveries(status, scheduled_at_ms, id);
CREATE INDEX idx_notification_deliveries_incident_episode_created
    ON notification_deliveries(incident_id, episode_number, created_at_ms DESC, id DESC);
CREATE INDEX idx_notification_deliveries_delivery_key
    ON notification_deliveries(delivery_key);

UPDATE persistence_schema_compatibility
SET schema_version = 29,
    updated_by_migration = 29,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 28;

CREATE TEMP TABLE persistence_v29_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 29)
);

INSERT INTO persistence_v29_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v29_schema_guard;
