-- Capacity-domain identity is an explicit local operator assertion. It is
-- deliberately independent of station URL, station type, account and key so
-- those mutable implementation details can never become a retry heuristic.
CREATE TABLE station_capacity_domains (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    provider_family TEXT NOT NULL CHECK (length(trim(provider_family)) BETWEEN 1 AND 128),
    deployment_identity TEXT CHECK (deployment_identity IS NULL OR length(trim(deployment_identity)) BETWEEN 1 AND 256),
    region_identity TEXT CHECK (region_identity IS NULL OR length(trim(region_identity)) BETWEEN 1 AND 128),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at TEXT NOT NULL
);

CREATE TRIGGER station_capacity_domains_revision_after_update
AFTER UPDATE OF provider_family, deployment_identity, region_identity ON station_capacity_domains
WHEN OLD.provider_family IS NOT NEW.provider_family
  OR OLD.deployment_identity IS NOT NEW.deployment_identity
  OR OLD.region_identity IS NOT NEW.region_identity
BEGIN
    UPDATE station_capacity_domains
    SET revision = OLD.revision + 1,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE station_id = NEW.station_id;
END;

UPDATE persistence_schema_compatibility
SET schema_version = 38,
    updated_by_migration = 38,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 38;
