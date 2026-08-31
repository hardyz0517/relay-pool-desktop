-- Canonical qualification reports make generation activation evidence
-- inspectable instead of retaining only detached digests. Reports are local
-- runtime artifacts and are rebuilt after portable import.

CREATE TABLE routing_generation_qualification_report (
    runtime_generation_id TEXT PRIMARY KEY
        REFERENCES routing_generation_qualification(runtime_generation_id) ON DELETE RESTRICT,
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

CREATE TRIGGER routing_generation_qualification_report_no_update
BEFORE UPDATE ON routing_generation_qualification_report
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification report is immutable');
END;

CREATE TRIGGER routing_generation_qualification_report_no_delete
BEFORE DELETE ON routing_generation_qualification_report
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification report is immutable');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 64,
    updated_by_migration = 64,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 64;

CREATE TEMP TABLE persistence_v64_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 64)
);
INSERT INTO persistence_v64_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v64_schema_guard;
