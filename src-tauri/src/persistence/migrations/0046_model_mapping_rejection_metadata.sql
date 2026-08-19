-- Preserve the complete reject action across restart. Existing rows are
-- policy rejects for backward compatibility; new documents write the exact
-- typed rejection kind and optional bounded message.
ALTER TABLE model_mapping_rules ADD COLUMN rejection_kind TEXT COLLATE BINARY
    CHECK (rejection_kind IS NULL OR rejection_kind IN ('unsupported_model', 'policy_denied', 'client_not_allowed'));
ALTER TABLE model_mapping_rules ADD COLUMN rejection_message TEXT
    CHECK (rejection_message IS NULL OR (length(CAST(rejection_message AS BLOB)) <= 256 AND instr(rejection_message, char(0)) = 0));

UPDATE persistence_schema_compatibility
SET schema_version = 46,
    updated_by_migration = 46,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 46;

CREATE TEMP TABLE persistence_v46_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 46)
);

INSERT INTO persistence_v46_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v46_schema_guard;
