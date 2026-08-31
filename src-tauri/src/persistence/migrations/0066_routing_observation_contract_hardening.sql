-- Harden the routing v3 evidence contract. Finalized request clusters are a
-- permanent fence, and quality generations retain the exact Key/probe-profile
-- eligibility snapshot used by their projector.

CREATE TABLE routing_attempt_late_audit_v3 (
    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('admission', 'terminal')),
    event_id TEXT NOT NULL CHECK (length(event_id) BETWEEN 1 AND 160),
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) BETWEEN 1 AND 160),
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) BETWEEN 1 AND 192),
    station_key_id TEXT CHECK (station_key_id IS NULL OR length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER CHECK (
        station_key_lifecycle_revision IS NULL OR station_key_lifecycle_revision > 0
    ),
    attempt_index INTEGER CHECK (attempt_index IS NULL OR attempt_index BETWEEN 0 AND 1023),
    reason_code TEXT NOT NULL CHECK (reason_code = 'late_after_finalization'),
    payload_commitment TEXT NOT NULL CHECK (length(payload_commitment) BETWEEN 1 AND 512),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (event_kind, event_id, payload_commitment)
);

CREATE INDEX idx_routing_attempt_late_audit_v3_cluster
    ON routing_attempt_late_audit_v3(
        correlation_id, event_kind, observed_at_ms, audit_id
    );

CREATE TRIGGER routing_attempt_late_audit_v3_no_update
BEFORE UPDATE ON routing_attempt_late_audit_v3
BEGIN
    SELECT RAISE(ABORT, 'routing late-attempt audit is immutable');
END;

CREATE TRIGGER routing_attempt_late_audit_v3_no_delete
BEFORE DELETE ON routing_attempt_late_audit_v3
BEGIN
    SELECT RAISE(ABORT, 'routing late-attempt audit is immutable');
END;

-- The store normally records the audit explicitly so it can return a typed
-- result. This trigger is the database-level race fence for direct or stale
-- writers: the late slot is dropped and the attempted identity is retained.
CREATE TRIGGER routing_attempt_v3_reject_late_admission
BEFORE INSERT ON routing_attempt_v3
WHEN EXISTS (
    SELECT 1
    FROM routing_attempt_cluster_v3 cluster
    WHERE cluster.source = NEW.source
      AND cluster.correlation_id = NEW.correlation_id
      AND cluster.cluster_finalized = 1
)
BEGIN
    INSERT OR IGNORE INTO routing_attempt_late_audit_v3 (
        event_kind, event_id, attempt_id, correlation_id, station_key_id,
        station_key_lifecycle_revision, attempt_index, reason_code,
        payload_commitment, observed_at_ms, created_at_ms
    ) VALUES (
        'admission', NEW.event_id, NEW.attempt_id, NEW.correlation_id,
        NEW.station_key_id, NEW.station_key_lifecycle_revision,
        NEW.attempt_index, 'late_after_finalization',
        NEW.event_id || ':' || NEW.correlation_id || ':' ||
        COALESCE(NEW.station_key_id, '') || ':' ||
        CAST(NEW.station_key_lifecycle_revision AS TEXT) || ':' ||
        CAST(NEW.attempt_index AS TEXT),
        NEW.created_at_ms, NEW.created_at_ms
    );
    SELECT RAISE(IGNORE);
END;

CREATE TRIGGER routing_attempt_cluster_v3_finalized_fence
BEFORE UPDATE ON routing_attempt_cluster_v3
WHEN OLD.cluster_finalized = 1
BEGIN
    SELECT RAISE(ABORT, 'finalized routing attempt cluster is immutable');
END;

CREATE TABLE routing_quality_source_profile_snapshot_v3 (
    snapshot_id TEXT PRIMARY KEY CHECK (length(snapshot_id) BETWEEN 5 AND 192),
    evaluation_at_ms INTEGER NOT NULL CHECK (evaluation_at_ms >= 0),
    input_observation_watermark INTEGER NOT NULL CHECK (input_observation_watermark >= 0),
    quality_policy_revision INTEGER NOT NULL CHECK (quality_policy_revision > 0),
    profile_count INTEGER NOT NULL CHECK (profile_count >= 0),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TABLE routing_quality_source_profile_snapshot_item_v3 (
    snapshot_id TEXT NOT NULL
        REFERENCES routing_quality_source_profile_snapshot_v3(snapshot_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    real_source_eligible INTEGER NOT NULL CHECK (real_source_eligible IN (0, 1)),
    monitoring_source_eligible INTEGER NOT NULL CHECK (monitoring_source_eligible IN (0, 1)),
    monitoring_profile_commitment TEXT CHECK (
        monitoring_profile_commitment IS NULL OR length(monitoring_profile_commitment) = 64
    ),
    captured_at_ms INTEGER NOT NULL CHECK (captured_at_ms >= 0),
    PRIMARY KEY (snapshot_id, station_key_id, station_key_lifecycle_revision),
    CHECK (monitoring_source_eligible = 1 OR monitoring_profile_commitment IS NULL)
);

CREATE INDEX idx_routing_quality_profile_snapshot_item_v3_key
    ON routing_quality_source_profile_snapshot_item_v3(
        snapshot_id, station_key_id, station_key_lifecycle_revision
    );

-- Snapshot items are staged before the immutable header inside one deferred-FK
-- transaction. Once the header exists, no later item can be appended.
CREATE TRIGGER routing_quality_source_profile_snapshot_item_v3_no_late_insert
BEFORE INSERT ON routing_quality_source_profile_snapshot_item_v3
WHEN EXISTS (
    SELECT 1 FROM routing_quality_source_profile_snapshot_v3 snapshot
    WHERE snapshot.snapshot_id = NEW.snapshot_id
)
BEGIN
    SELECT RAISE(ABORT, 'routing quality source profile snapshot is finalized');
END;

CREATE TRIGGER routing_quality_source_profile_snapshot_v3_no_update
BEFORE UPDATE ON routing_quality_source_profile_snapshot_v3
BEGIN
    SELECT RAISE(ABORT, 'routing quality source profile snapshot is immutable');
END;

CREATE TRIGGER routing_quality_source_profile_snapshot_v3_no_delete
BEFORE DELETE ON routing_quality_source_profile_snapshot_v3
BEGIN
    SELECT RAISE(ABORT, 'routing quality source profile snapshot is immutable');
END;

CREATE TRIGGER routing_quality_source_profile_snapshot_item_v3_no_update
BEFORE UPDATE ON routing_quality_source_profile_snapshot_item_v3
BEGIN
    SELECT RAISE(ABORT, 'routing quality source profile snapshot item is immutable');
END;

CREATE TRIGGER routing_quality_source_profile_snapshot_item_v3_no_delete
BEFORE DELETE ON routing_quality_source_profile_snapshot_item_v3
BEGIN
    SELECT RAISE(ABORT, 'routing quality source profile snapshot item is immutable');
END;

ALTER TABLE routing_quality_generation_v3
    ADD COLUMN source_profile_snapshot_id TEXT
        REFERENCES routing_quality_source_profile_snapshot_v3(snapshot_id) ON DELETE RESTRICT;

CREATE TRIGGER routing_quality_generation_v3_profile_snapshot_immutable
BEFORE UPDATE OF source_profile_snapshot_id ON routing_quality_generation_v3
WHEN OLD.source_profile_snapshot_id IS NOT NEW.source_profile_snapshot_id
BEGIN
    SELECT RAISE(ABORT, 'routing quality generation profile snapshot is immutable');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 66,
    updated_by_migration = 66,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 66;

CREATE TEMP TABLE persistence_v66_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 66)
);
INSERT INTO persistence_v66_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v66_schema_guard;
