-- Durable intent for the collection that must follow a successful WebView
-- authorization.  This table is deliberately narrow: it is not a general
-- message queue and contains no credential material.
CREATE TABLE post_authorization_collection_work (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at_ms INTEGER NOT NULL CHECK (next_attempt_at_ms >= 0),
    last_error_code TEXT,
    operation_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE INDEX idx_post_authorization_work_due
    ON post_authorization_collection_work(state, next_attempt_at_ms, station_id);

-- Retire stale collection rollups from the compatibility column.  From this
-- migration onward it carries only the administrative enabled/disabled
-- baseline; collection health is read from the typed projection below.
UPDATE stations
SET status = CASE WHEN enabled = 0 THEN 'disabled' ELSE 'unchecked' END
WHERE status IS NULL
   OR status NOT IN ('disabled', 'unchecked');

CREATE TABLE station_authorization_projection (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('unknown', 'verifying', 'valid', 'reauthorization_required', 'indeterminate')),
    credential_revision INTEGER NOT NULL CHECK (credential_revision >= 0),
    intent_sequence INTEGER NOT NULL CHECK (intent_sequence >= 0),
    authority TEXT NOT NULL,
    reason_code TEXT,
    operation_id TEXT NOT NULL,
    source_operation_id TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE station_collection_projection (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('not_collected', 'collecting', 'healthy', 'degraded', 'failed', 'stale')),
    reason_codes_json TEXT NOT NULL CHECK (json_valid(reason_codes_json)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    intent_sequence INTEGER NOT NULL CHECK (intent_sequence > 0),
    operation_id TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

-- Station Asset is a workspace read model with one durable revision owner.
-- These triggers are deliberately attached to the tables read by
-- StationAssetsQuery so every mutation advances the family revision in the
-- same transaction, including low-level imports, station reorder/delete, and
-- future writers that do not know about the application event bridge.
INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
VALUES (
    'read_model:station_assets',
    1,
    CAST(strftime('%s', 'now') AS INTEGER) * 1000,
    'baseline_snapshot'
)
ON CONFLICT(scope) DO NOTHING;

CREATE TRIGGER station_assets_revision_stations_insert
AFTER INSERT ON stations
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_stations_update
AFTER UPDATE ON stations
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_stations_delete
AFTER DELETE ON stations
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_keys_insert
AFTER INSERT ON station_keys
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_keys_update
AFTER UPDATE ON station_keys
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_keys_delete
AFTER DELETE ON station_keys
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_key_capabilities_insert
AFTER INSERT ON station_key_capabilities
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_key_capabilities_update
AFTER UPDATE ON station_key_capabilities
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_key_capabilities_delete
AFTER DELETE ON station_key_capabilities
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_endpoint_health_insert
AFTER INSERT ON endpoint_health_snapshot
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_endpoint_health_update
AFTER UPDATE ON endpoint_health_snapshot
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_endpoint_health_delete
AFTER DELETE ON endpoint_health_snapshot
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_authorization_insert
AFTER INSERT ON station_authorization_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_authorization_update
AFTER UPDATE ON station_authorization_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_authorization_delete
AFTER DELETE ON station_authorization_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_collection_insert
AFTER INSERT ON station_collection_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_collection_update
AFTER UPDATE ON station_collection_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_collection_delete
AFTER DELETE ON station_collection_projection
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_secrets_insert
AFTER INSERT ON secrets
WHEN NEW.scope IN ('station', 'station_key', 'station_credentials')
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_secrets_update
AFTER UPDATE ON secrets
WHEN OLD.scope IN ('station', 'station_key', 'station_credentials')
  OR NEW.scope IN ('station', 'station_key', 'station_credentials')
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

CREATE TRIGGER station_assets_revision_station_secrets_delete
AFTER DELETE ON secrets
WHEN OLD.scope IN ('station', 'station_key', 'station_credentials')
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_assets';
END;

UPDATE persistence_schema_compatibility
SET schema_version = 72,
    updated_by_migration = 72,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 72;

CREATE TEMP TABLE persistence_v72_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 72)
);
INSERT INTO persistence_v72_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v72_schema_guard;
