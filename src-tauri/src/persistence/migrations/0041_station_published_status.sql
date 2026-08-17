-- Official station-published status is a collector-owned display fact. It is
-- intentionally separate from active channel monitoring and routing health.
CREATE TABLE station_published_status_sources (
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision >= 1),
    source_kind TEXT NOT NULL CHECK (length(CAST(source_kind AS BLOB)) BETWEEN 1 AND 64),
    source_state TEXT NOT NULL CHECK (source_state IN (
        'never_collected',
        'available',
        'empty',
        'unsupported',
        'authorization_required',
        'degraded',
        'failed'
    )),
    last_attempt_at TEXT NOT NULL,
    last_success_at TEXT,
    last_complete_at TEXT,
    last_error_kind TEXT,
    monitor_count INTEGER NOT NULL DEFAULT 0 CHECK (monitor_count BETWEEN 0 AND 512),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (station_id, endpoint_revision, source_kind)
);

CREATE INDEX idx_station_published_status_sources_station_revision
    ON station_published_status_sources(station_id, endpoint_revision, source_kind);

CREATE TABLE station_published_monitors (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision >= 1),
    source_kind TEXT NOT NULL CHECK (length(CAST(source_kind AS BLOB)) BETWEEN 1 AND 64),
    upstream_monitor_id TEXT NOT NULL CHECK (length(CAST(upstream_monitor_id AS BLOB)) BETWEEN 1 AND 256),
    identity_kind TEXT NOT NULL CHECK (identity_kind IN ('upstream_id', 'derived_fallback')),
    name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 512),
    provider TEXT NOT NULL CHECK (length(CAST(provider AS BLOB)) BETWEEN 1 AND 256),
    group_name TEXT CHECK (group_name IS NULL OR length(CAST(group_name AS BLOB)) <= 256),
    primary_model TEXT NOT NULL CHECK (length(CAST(primary_model AS BLOB)) BETWEEN 1 AND 512),
    extra_models_json TEXT NOT NULL CHECK (
        json_valid(extra_models_json)
        AND json_type(extra_models_json) = 'array'
        AND length(CAST(extra_models_json AS BLOB)) <= 8192
    ),
    presence_status TEXT NOT NULL CHECK (presence_status IN ('current', 'missing')),
    current_outcome TEXT NOT NULL CHECK (current_outcome IN ('available', 'degraded', 'unavailable', 'unknown')),
    source_status TEXT NOT NULL CHECK (length(CAST(source_status AS BLOB)) <= 64),
    current_latency_ms INTEGER CHECK (current_latency_ms IS NULL OR current_latency_ms >= 0),
    current_ping_latency_ms INTEGER CHECK (current_ping_latency_ms IS NULL OR current_ping_latency_ms >= 0),
    availability_7d_percent REAL CHECK (
        availability_7d_percent IS NULL
        OR (availability_7d_percent >= 0 AND availability_7d_percent <= 100)
    ),
    upstream_checked_at TEXT,
    last_seen_run_id TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (station_id, endpoint_revision, source_kind, upstream_monitor_id)
);

CREATE INDEX idx_station_published_monitors_workspace
    ON station_published_monitors(
        station_id,
        endpoint_revision,
        source_kind,
        presence_status,
        updated_at DESC,
        id DESC
    );

CREATE TABLE station_published_monitor_samples (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES station_published_monitors(id) ON DELETE CASCADE,
    model TEXT NOT NULL CHECK (length(CAST(model AS BLOB)) BETWEEN 1 AND 512),
    checked_at TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('available', 'degraded', 'unavailable', 'unknown')),
    source_status TEXT NOT NULL CHECK (length(CAST(source_status AS BLOB)) <= 64),
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    ping_latency_ms INTEGER CHECK (ping_latency_ms IS NULL OR ping_latency_ms >= 0),
    safe_message TEXT CHECK (safe_message IS NULL OR length(CAST(safe_message AS BLOB)) <= 512),
    first_seen_run_id TEXT NOT NULL,
    last_seen_run_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (monitor_id, model, checked_at)
);

CREATE INDEX idx_station_published_monitor_samples_timeline
    ON station_published_monitor_samples(monitor_id, model, checked_at DESC, id DESC);

INSERT OR IGNORE INTO settings (key, value, updated_at)
VALUES ('published_status_interval_minutes', '5', strftime('%s', 'now'));

UPDATE persistence_schema_compatibility
SET schema_version = 41,
    updated_by_migration = 41,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 40;

CREATE TEMP TABLE persistence_v41_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 41)
);

INSERT INTO persistence_v41_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v41_schema_guard;
