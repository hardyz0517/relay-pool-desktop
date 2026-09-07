-- Preserve provider-supplied group descriptions as nullable display metadata.
-- Descriptions are deliberately not part of group identity or routing hashes.
ALTER TABLE station_group_bindings
ADD COLUMN description TEXT
    CHECK (description IS NULL OR (length(CAST(description AS BLOB)) <= 1024 AND instr(description, char(0)) = 0));

ALTER TABLE group_rate_records
ADD COLUMN description TEXT
    CHECK (description IS NULL OR (length(CAST(description AS BLOB)) <= 1024 AND instr(description, char(0)) = 0));

UPDATE persistence_schema_compatibility
SET schema_version = 77,
    updated_by_migration = 77,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
