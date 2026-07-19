ALTER TABLE station_keys ADD COLUMN name TEXT NOT NULL DEFAULT 'Default Key';
ALTER TABLE station_keys ADD COLUMN api_key TEXT NOT NULL DEFAULT '';
ALTER TABLE station_keys ADD COLUMN api_key_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL;
ALTER TABLE station_keys ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1));
ALTER TABLE station_keys ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE station_keys ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 3 CHECK (max_concurrency > 0);
ALTER TABLE station_keys ADD COLUMN load_factor INTEGER;
ALTER TABLE station_keys ADD COLUMN schedulable INTEGER NOT NULL DEFAULT 1 CHECK (schedulable IN (0, 1));
ALTER TABLE station_keys ADD COLUMN group_name TEXT;
ALTER TABLE station_keys ADD COLUMN tier_label TEXT;
ALTER TABLE station_keys ADD COLUMN group_binding_id TEXT;
ALTER TABLE station_keys ADD COLUMN group_id_hash TEXT;
ALTER TABLE station_keys ADD COLUMN rate_multiplier REAL;
ALTER TABLE station_keys ADD COLUMN manual_rate_multiplier REAL;
ALTER TABLE station_keys ADD COLUMN manual_rate_updated_at TEXT;
ALTER TABLE station_keys ADD COLUMN rate_source TEXT;
ALTER TABLE station_keys ADD COLUMN rate_collected_at TEXT;
ALTER TABLE station_keys ADD COLUMN balance_scope TEXT;
ALTER TABLE station_keys ADD COLUMN status TEXT NOT NULL DEFAULT 'unchecked';
ALTER TABLE station_keys ADD COLUMN last_checked_at TEXT;
ALTER TABLE station_keys ADD COLUMN last_used_at TEXT;
ALTER TABLE station_keys ADD COLUMN note TEXT;
ALTER TABLE station_keys ADD COLUMN created_at TEXT NOT NULL DEFAULT '0';
ALTER TABLE station_keys ADD COLUMN updated_at TEXT NOT NULL DEFAULT '0';

CREATE INDEX idx_station_keys_order ON station_keys(station_id, priority, created_at);

CREATE TABLE remote_station_keys (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    remote_key_id_hash TEXT,
    remote_key_name TEXT,
    api_key_masked TEXT,
    api_key_fingerprint TEXT,
    group_id_hash TEXT,
    group_name TEXT,
    tier_label TEXT,
    rate_multiplier REAL,
    rate_source TEXT,
    created_at TEXT,
    last_used_at TEXT,
    raw_source TEXT NOT NULL DEFAULT 'collector',
    match_status TEXT NOT NULL DEFAULT 'unbound',
    matched_station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    match_confidence REAL NOT NULL DEFAULT 0.0,
    collected_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(station_id, remote_key_id_hash)
);

UPDATE persistence_schema_compatibility
SET schema_version = 3,
    updated_by_migration = 3,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
