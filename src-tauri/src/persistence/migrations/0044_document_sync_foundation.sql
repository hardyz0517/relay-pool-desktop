-- Shared, coalescing document-sync projection. This is intentionally not a
-- FIFO outbox and does not store raw invalid file contents.
CREATE TABLE routing_document_sync (
    document_kind TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(document_kind AS BLOB)) BETWEEN 1 AND 128
            AND instr(document_kind, char(0)) = 0),
    desired_revision INTEGER NOT NULL CHECK (desired_revision > 0),
    desired_canonical_digest TEXT
        CHECK (desired_canonical_digest IS NULL
            OR (length(CAST(desired_canonical_digest AS BLOB)) = 64
                AND desired_canonical_digest NOT GLOB '*[^0-9A-Fa-f]*')),
    materialized_revision INTEGER
        CHECK (materialized_revision IS NULL OR materialized_revision > 0),
    materialized_canonical_digest TEXT
        CHECK (materialized_canonical_digest IS NULL
            OR (length(CAST(materialized_canonical_digest AS BLOB)) = 64
                AND materialized_canonical_digest NOT GLOB '*[^0-9A-Fa-f]*')),
    sync_state TEXT NOT NULL COLLATE BINARY
        CHECK (sync_state IN ('synchronized', 'pending_materialization', 'external_change', 'error')),
    last_observed_raw_digest TEXT
        CHECK (last_observed_raw_digest IS NULL
            OR (length(CAST(last_observed_raw_digest AS BLOB)) = 64
                AND last_observed_raw_digest NOT GLOB '*[^0-9A-Fa-f]*')),
    last_error_code TEXT
        CHECK (last_error_code IS NULL
            OR (length(CAST(last_error_code AS BLOB)) BETWEEN 1 AND 96
                AND instr(last_error_code, char(0)) = 0)),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    attempt_token TEXT,
    lease_owner TEXT,
    lease_expires_at_ms INTEGER
        CHECK (lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

INSERT INTO routing_document_sync (
    document_kind, desired_revision, desired_canonical_digest,
    materialized_revision, materialized_canonical_digest, sync_state,
    last_observed_raw_digest, last_error_code, retry_count,
    attempt_token, lease_owner, lease_expires_at_ms, updated_at_ms
)
SELECT 'routing_policy', config_revision, NULL, NULL, NULL,
       'pending_materialization', NULL, NULL, 0, NULL, NULL, NULL, updated_at_ms
FROM routing_policy
WHERE singleton_key = 1;

INSERT INTO routing_document_sync (
    document_kind, desired_revision, desired_canonical_digest,
    materialized_revision, materialized_canonical_digest, sync_state,
    last_observed_raw_digest, last_error_code, retry_count,
    attempt_token, lease_owner, lease_expires_at_ms, updated_at_ms
)
SELECT 'model_mapping', revision, NULL, NULL, NULL,
       'pending_materialization', NULL, NULL, 0, NULL, NULL, NULL, updated_at_ms
FROM model_mapping_policies
WHERE singleton_key = 1;

UPDATE persistence_schema_compatibility
SET schema_version = 44,
    updated_by_migration = 44,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 44;
