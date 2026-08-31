-- Durable preparation identity for resumable quality generation builds and a
-- local-only HMAC key for redacted qualification reports.

ALTER TABLE routing_quality_generation_v3 ADD COLUMN build_request_hash TEXT
    CHECK (build_request_hash IS NULL OR length(build_request_hash) = 64);
ALTER TABLE routing_quality_generation_v3 ADD COLUMN expected_input_observation_count INTEGER
    CHECK (expected_input_observation_count IS NULL OR expected_input_observation_count >= 0);
ALTER TABLE routing_quality_generation_v3 ADD COLUMN expected_output_scope_count INTEGER
    CHECK (expected_output_scope_count IS NULL OR expected_output_scope_count >= 0);

CREATE UNIQUE INDEX idx_routing_quality_generation_v3_build_request
    ON routing_quality_generation_v3(build_request_hash)
    WHERE build_request_hash IS NOT NULL;

CREATE TRIGGER routing_quality_generation_v3_resume_identity_immutable
BEFORE UPDATE OF build_request_hash, expected_input_observation_count,
    expected_output_scope_count
ON routing_quality_generation_v3
WHEN OLD.build_request_hash IS NOT NEW.build_request_hash
  OR OLD.expected_input_observation_count IS NOT NEW.expected_input_observation_count
  OR OLD.expected_output_scope_count IS NOT NEW.expected_output_scope_count
BEGIN
    SELECT RAISE(ABORT, 'routing quality generation resume identity is immutable');
END;

CREATE TABLE routing_generation_report_secret (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    secret BLOB NOT NULL CHECK (length(secret) = 32),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TRIGGER routing_generation_report_secret_immutable_update
BEFORE UPDATE ON routing_generation_report_secret
BEGIN
    SELECT RAISE(ABORT, 'routing generation report secret is immutable');
END;

CREATE TRIGGER routing_generation_report_secret_immutable_delete
BEFORE DELETE ON routing_generation_report_secret
BEGIN
    SELECT RAISE(ABORT, 'routing generation report secret is immutable');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 67,
    updated_by_migration = 67,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 67;

CREATE TEMP TABLE persistence_v67_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 67)
);
INSERT INTO persistence_v67_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v67_schema_guard;
