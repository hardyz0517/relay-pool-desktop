-- Qualification v2 has a stricter, inspectable report contract than the
-- original digest-only qualification. Keep the v1 tables as historical local
-- runtime evidence and publish a separate immutable v2 authority.

CREATE TABLE routing_generation_qualification_v2 (
    runtime_generation_id TEXT PRIMARY KEY
        REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    qualification_version TEXT NOT NULL
        CHECK (qualification_version = 'routing-generation-qualification-v2'),
    comparison_status TEXT NOT NULL CHECK (comparison_status = 'passed'),
    comparison_report_hash TEXT NOT NULL CHECK (length(comparison_report_hash) = 64),
    replay_status TEXT NOT NULL CHECK (replay_status = 'passed'),
    replay_report_hash TEXT NOT NULL CHECK (length(replay_report_hash) = 64),
    qualified_at_ms INTEGER NOT NULL CHECK (qualified_at_ms >= 0)
);

CREATE TRIGGER routing_generation_qualification_v2_no_update
BEFORE UPDATE ON routing_generation_qualification_v2
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification v2 is immutable');
END;

CREATE TRIGGER routing_generation_qualification_v2_no_delete
BEFORE DELETE ON routing_generation_qualification_v2
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification v2 is immutable');
END;

CREATE TABLE routing_generation_qualification_report_v2 (
    runtime_generation_id TEXT PRIMARY KEY
        REFERENCES routing_generation_qualification_v2(runtime_generation_id) ON DELETE RESTRICT,
    comparison_report_json TEXT NOT NULL CHECK (
        json_valid(comparison_report_json)
        AND json_type(comparison_report_json) = 'object'
    ),
    comparison_report_hash TEXT NOT NULL CHECK (length(comparison_report_hash) = 64),
    replay_report_json TEXT NOT NULL CHECK (
        json_valid(replay_report_json)
        AND json_type(replay_report_json) = 'object'
    ),
    replay_report_hash TEXT NOT NULL CHECK (length(replay_report_hash) = 64),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TRIGGER routing_generation_qualification_report_v2_no_update
BEFORE UPDATE ON routing_generation_qualification_report_v2
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification report v2 is immutable');
END;

CREATE TRIGGER routing_generation_qualification_report_v2_no_delete
BEFORE DELETE ON routing_generation_qualification_report_v2
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification report v2 is immutable');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 69,
    updated_by_migration = 69,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 69;

CREATE TEMP TABLE persistence_v69_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 69)
);
INSERT INTO persistence_v69_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v69_schema_guard;
