-- The routing foundation is additive. Existing routing behavior continues to
-- use its current tables until the atomic cutover; these facts make that
-- cutover able to reject an unknown generation instead of guessing one.
CREATE TABLE IF NOT EXISTS domain_revisions (
    scope TEXT PRIMARY KEY CHECK (length(scope) BETWEEN 1 AND 192),
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    provenance TEXT NOT NULL CHECK (provenance IN (
        'legacy_endpoint_revision',
        'baseline_snapshot',
        'transactional_write'
    ))
);

-- A station endpoint revision is the only pre-foundation durable revision.
-- All other legacy records receive an explicit snapshot baseline rather than
-- deriving a revision from a timestamp or silently assigning a fallback.
INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT
    'station:' || id,
    CASE WHEN endpoint_revision > 0 THEN endpoint_revision ELSE 1 END,
    0,
    CASE
        WHEN endpoint_revision > 0 THEN 'legacy_endpoint_revision'
        ELSE 'baseline_snapshot'
    END
FROM stations
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'station_key:' || id, row_number, 0, 'baseline_snapshot'
FROM (
    SELECT id, ROW_NUMBER() OVER (ORDER BY id ASC) AS row_number
    FROM station_keys
)
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'setting:' || key, row_number, 0, 'baseline_snapshot'
FROM (
    SELECT key, ROW_NUMBER() OVER (ORDER BY key ASC) AS row_number
    FROM settings
)
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'model_alias:' || id, row_number, 0, 'baseline_snapshot'
FROM (
    SELECT id, ROW_NUMBER() OVER (ORDER BY id ASC) AS row_number
    FROM model_aliases
)
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
VALUES ('routing_policy', 1, 0, 'baseline_snapshot')
ON CONFLICT(scope) DO NOTHING;

CREATE TABLE IF NOT EXISTS routing_policy (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    config_revision INTEGER NOT NULL CHECK (config_revision > 0),
    policy_version TEXT NOT NULL CHECK (length(policy_version) BETWEEN 1 AND 96),
    system_version TEXT NOT NULL CHECK (length(system_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN ('routing_configuration_required', 'active', 'invalid')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS routing_policy_history (
    config_revision INTEGER PRIMARY KEY CHECK (config_revision > 0),
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    policy_version TEXT NOT NULL CHECK (length(policy_version) BETWEEN 1 AND 96),
    system_version TEXT NOT NULL CHECK (length(system_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN ('routing_configuration_required', 'active', 'invalid')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS routing_observations (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 160),
    producer_id TEXT NOT NULL CHECK (length(producer_id) BETWEEN 1 AND 96),
    producer_sequence INTEGER NOT NULL CHECK (producer_sequence >= 0),
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) BETWEEN 16 AND 128),
    event_at_ms INTEGER NOT NULL CHECK (event_at_ms >= 0),
    ingested_at_ms INTEGER NOT NULL CHECK (ingested_at_ms >= 0),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    source TEXT NOT NULL CHECK (length(source) BETWEEN 1 AND 64),
    traffic_equivalence TEXT NOT NULL CHECK (length(traffic_equivalence) BETWEEN 1 AND 96),
    outcome_kind TEXT NOT NULL CHECK (length(outcome_kind) BETWEEN 1 AND 64),
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    mass_basis_points INTEGER CHECK (mass_basis_points IS NULL OR mass_basis_points BETWEEN 0 AND 10000),
    evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (producer_id, producer_sequence)
);

CREATE INDEX IF NOT EXISTS idx_routing_observations_scope_order
    ON routing_observations(scope, event_at_ms ASC, id ASC);

CREATE TABLE IF NOT EXISTS routing_projector_checkpoints (
    projector TEXT NOT NULL CHECK (length(projector) BETWEEN 1 AND 96),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    checkpoint_sequence INTEGER NOT NULL CHECK (checkpoint_sequence >= 0),
    status TEXT NOT NULL CHECK (status IN ('ready', 'projecting', 'failed')),
    error_code TEXT,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (projector, projector_version, scope)
);

CREATE TABLE IF NOT EXISTS routing_quality_summaries (
    scope TEXT PRIMARY KEY CHECK (length(scope) BETWEEN 1 AND 192),
    quality_revision INTEGER NOT NULL CHECK (quality_revision > 0),
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE IF NOT EXISTS routing_health_axes (
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    axis TEXT NOT NULL CHECK (axis IN ('availability', 'latency', 'reliability', 'freshness')),
    health_revision INTEGER NOT NULL CHECK (health_revision > 0),
    value_basis_points INTEGER NOT NULL CHECK (value_basis_points BETWEEN 0 AND 10000),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (scope, axis)
);

UPDATE persistence_schema_compatibility
SET schema_version = 24,
    updated_by_migration = 24,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 24;
