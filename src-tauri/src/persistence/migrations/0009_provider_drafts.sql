CREATE TABLE provider_drafts (
    id TEXT PRIMARY KEY,
    base_station_id TEXT REFERENCES stations(id) ON DELETE SET NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'committed')),
    payload_schema_version INTEGER NOT NULL DEFAULT 1 CHECK (payload_schema_version >= 1),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    commit_key TEXT UNIQUE,
    committed_station_id TEXT REFERENCES stations(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_provider_drafts_active_updated
    ON provider_drafts(state, updated_at DESC, id DESC);

CREATE UNIQUE INDEX idx_provider_drafts_single_active_create
    ON provider_drafts((1))
    WHERE state = 'active' AND base_station_id IS NULL;

CREATE TABLE provider_draft_previews (
    draft_id TEXT NOT NULL REFERENCES provider_drafts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('detect', 'balance', 'groups', 'models', 'full', 'remote_keys', 'capture')),
    runtime_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL,
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    collected_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (draft_id, kind)
);

UPDATE persistence_schema_compatibility
SET schema_version = 9,
    updated_by_migration = 9,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
