-- Native capability identity bridge.
--
-- Rows written before the mapping cutover keep the legacy composite primary
-- key (including model_alias_revision). New rows use identity_version = 2 and
-- are deduplicated by the native model plus endpoint/protocol and execution
-- revisions. The legacy column remains durable provenance only.
CREATE UNIQUE INDEX idx_routing_capability_model_native_identity_v2
    ON routing_capability_model_verdicts(
        station_key_id,
        resolved_model,
        endpoint_kind,
        protocol_kind,
        credential_revision,
        endpoint_revision
    )
    WHERE identity_version >= 2;

UPDATE persistence_schema_compatibility
SET schema_version = 45,
    updated_by_migration = 45,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 45;

CREATE TEMP TABLE persistence_v45_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 45)
);

INSERT INTO persistence_v45_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v45_schema_guard;
