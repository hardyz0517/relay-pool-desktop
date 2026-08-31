-- Fail-closed station-key circuit persistence gate. Request traffic can open
-- a gate, but only the supervised read/write health check may clear it.

CREATE TABLE routing_circuit_persistence_gate_v3 (
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL
        CHECK (station_key_lifecycle_revision > 0),
    status TEXT NOT NULL CHECK (status = 'persistence_unavailable'),
    reason_code TEXT NOT NULL CHECK (reason_code = 'circuit_persistence_unavailable'),
    opened_at_ms INTEGER NOT NULL CHECK (opened_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= opened_at_ms),
    PRIMARY KEY (station_key_id, station_key_lifecycle_revision)
);

CREATE INDEX idx_routing_circuit_persistence_gate_v3_status
    ON routing_circuit_persistence_gate_v3(status, updated_at_ms);

CREATE TABLE routing_circuit_persistence_health_v3 (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    check_revision INTEGER NOT NULL CHECK (check_revision >= 0),
    checked_at_ms INTEGER CHECK (checked_at_ms IS NULL OR checked_at_ms >= 0)
);

INSERT INTO routing_circuit_persistence_health_v3 (
    singleton_key, check_revision, checked_at_ms
) VALUES (1, 0, NULL);

UPDATE persistence_schema_compatibility
SET schema_version = 68,
    updated_by_migration = 68,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 68;

CREATE TEMP TABLE persistence_v68_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 68)
);
INSERT INTO persistence_v68_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v68_schema_guard;
