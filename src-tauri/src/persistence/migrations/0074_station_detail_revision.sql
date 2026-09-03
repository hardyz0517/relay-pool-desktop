-- Station Detail owns one monotonic revision per station. Every table read by
-- StationDetailQuery advances that station's row in the same write transaction.

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'read_model:station_detail:' || id, 1,
       CAST(strftime('%s', 'now') AS INTEGER) * 1000, 'baseline_snapshot'
FROM stations
WHERE 1
ON CONFLICT(scope) DO NOTHING;

CREATE TRIGGER station_detail_revision_stations_insert
AFTER INSERT ON stations
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('read_model:station_detail:' || NEW.id, 1,
            CAST(strftime('%s', 'now') AS INTEGER) * 1000, 'transactional_write')
    ON CONFLICT(scope) DO UPDATE SET
        revision = revision + 1, updated_at_ms = excluded.updated_at_ms,
        provenance = excluded.provenance;
END;

CREATE TRIGGER station_detail_revision_stations_update
AFTER UPDATE ON stations
BEGIN
    UPDATE domain_revisions
    SET revision = revision + 1,
        updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        provenance = 'transactional_write'
    WHERE scope = 'read_model:station_detail:' || NEW.id;
END;

CREATE TRIGGER station_detail_revision_stations_delete
AFTER DELETE ON stations
BEGIN
    DELETE FROM domain_revisions WHERE scope = 'read_model:station_detail:' || OLD.id;
END;

CREATE TRIGGER station_detail_revision_station_keys_insert AFTER INSERT ON station_keys BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_station_keys_update AFTER UPDATE ON station_keys BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_station_keys_delete AFTER DELETE ON station_keys BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_key_capabilities_insert AFTER INSERT ON station_key_capabilities BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || (SELECT station_id FROM station_keys WHERE id = NEW.station_key_id);
END;
CREATE TRIGGER station_detail_revision_key_capabilities_update AFTER UPDATE ON station_key_capabilities BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || (SELECT station_id FROM station_keys WHERE id = NEW.station_key_id);
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_key_id <> NEW.station_key_id AND scope = 'read_model:station_detail:' || (SELECT station_id FROM station_keys WHERE id = OLD.station_key_id);
END;
CREATE TRIGGER station_detail_revision_key_capabilities_delete AFTER DELETE ON station_key_capabilities BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || (SELECT station_id FROM station_keys WHERE id = OLD.station_key_id);
END;

CREATE TRIGGER station_detail_revision_endpoint_health_insert AFTER INSERT ON endpoint_health_snapshot BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_endpoint_health_update AFTER UPDATE ON endpoint_health_snapshot BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_endpoint_health_delete AFTER DELETE ON endpoint_health_snapshot BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_authorization_insert AFTER INSERT ON station_authorization_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_authorization_update AFTER UPDATE ON station_authorization_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_authorization_delete AFTER DELETE ON station_authorization_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_collection_insert AFTER INSERT ON station_collection_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_collection_update AFTER UPDATE ON station_collection_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_collection_delete AFTER DELETE ON station_collection_projection BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_credentials_insert AFTER INSERT ON station_credentials BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_credentials_update AFTER UPDATE ON station_credentials BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_credentials_delete AFTER DELETE ON station_credentials BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_station_secrets_insert AFTER INSERT ON secrets
WHEN NEW.scope IN ('station', 'station_key', 'station_credentials') BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || CASE WHEN NEW.scope = 'station_key' THEN (SELECT station_id FROM station_keys WHERE id = NEW.owner_id) ELSE NEW.owner_id END;
END;
CREATE TRIGGER station_detail_revision_station_secrets_update AFTER UPDATE ON secrets
WHEN OLD.scope IN ('station', 'station_key', 'station_credentials') OR NEW.scope IN ('station', 'station_key', 'station_credentials') BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || CASE WHEN NEW.scope = 'station_key' THEN (SELECT station_id FROM station_keys WHERE id = NEW.owner_id) ELSE NEW.owner_id END;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write'
    WHERE (OLD.scope <> NEW.scope OR OLD.owner_id <> NEW.owner_id)
      AND scope = 'read_model:station_detail:' || CASE WHEN OLD.scope = 'station_key' THEN (SELECT station_id FROM station_keys WHERE id = OLD.owner_id) ELSE OLD.owner_id END;
END;
CREATE TRIGGER station_detail_revision_station_secrets_delete AFTER DELETE ON secrets
WHEN OLD.scope IN ('station', 'station_key', 'station_credentials') BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || CASE WHEN OLD.scope = 'station_key' THEN (SELECT station_id FROM station_keys WHERE id = OLD.owner_id) ELSE OLD.owner_id END;
END;

CREATE TRIGGER station_detail_revision_collector_runs_insert AFTER INSERT ON collector_runs BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_collector_runs_update AFTER UPDATE ON collector_runs BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_collector_runs_delete AFTER DELETE ON collector_runs BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_collector_snapshots_insert AFTER INSERT ON collector_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_collector_snapshots_update AFTER UPDATE ON collector_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_collector_snapshots_delete AFTER DELETE ON collector_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_group_bindings_insert AFTER INSERT ON station_group_bindings BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_group_bindings_update AFTER UPDATE ON station_group_bindings BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_group_bindings_delete AFTER DELETE ON station_group_bindings BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_group_rates_insert AFTER INSERT ON group_rate_records BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_group_rates_update AFTER UPDATE ON group_rate_records BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_group_rates_delete AFTER DELETE ON group_rate_records BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_balances_insert AFTER INSERT ON balance_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_balances_update AFTER UPDATE ON balance_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id <> NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_balances_delete AFTER DELETE ON balance_snapshots BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

CREATE TRIGGER station_detail_revision_incidents_insert AFTER INSERT ON change_incidents WHEN NEW.station_id IS NOT NULL BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || NEW.station_id;
END;
CREATE TRIGGER station_detail_revision_incidents_update AFTER UPDATE ON change_incidents WHEN OLD.station_id IS NOT NULL OR NEW.station_id IS NOT NULL BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE NEW.station_id IS NOT NULL AND scope = 'read_model:station_detail:' || NEW.station_id;
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE OLD.station_id IS NOT NULL AND OLD.station_id IS NOT NEW.station_id AND scope = 'read_model:station_detail:' || OLD.station_id;
END;
CREATE TRIGGER station_detail_revision_incidents_delete AFTER DELETE ON change_incidents WHEN OLD.station_id IS NOT NULL BEGIN
    UPDATE domain_revisions SET revision = revision + 1, updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000, provenance = 'transactional_write' WHERE scope = 'read_model:station_detail:' || OLD.station_id;
END;

UPDATE persistence_schema_compatibility
SET schema_version = 74, updated_by_migration = 74,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version < 74;

CREATE TEMP TABLE persistence_v74_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 74)
);
INSERT INTO persistence_v74_schema_guard (schema_version)
SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1;
DROP TABLE persistence_v74_schema_guard;
