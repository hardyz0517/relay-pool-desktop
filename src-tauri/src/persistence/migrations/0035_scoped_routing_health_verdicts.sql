-- Scoped routing admission is derived from immutable, typed observations.
-- Generations make rebuild/cutover atomic: the planner always reads one
-- complete active generation and never observes DELETE + replay gaps.
CREATE TABLE routing_health_generations (
    generation_id TEXT PRIMARY KEY CHECK (length(generation_id) BETWEEN 1 AND 96),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN ('shadow', 'active', 'retired', 'failed')),
    watermark_ingested_at_ms INTEGER CHECK (watermark_ingested_at_ms IS NULL OR watermark_ingested_at_ms >= 0),
    watermark_ingestion_sequence INTEGER CHECK (watermark_ingestion_sequence IS NULL OR watermark_ingestion_sequence > 0),
    watermark_observation_id TEXT CHECK (watermark_observation_id IS NULL OR length(watermark_observation_id) BETWEEN 1 AND 160),
    projected_row_count INTEGER NOT NULL DEFAULT 0 CHECK (projected_row_count >= 0),
    projected_content_hash TEXT CHECK (projected_content_hash IS NULL OR length(projected_content_hash) = 64),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    activated_at_ms INTEGER CHECK (activated_at_ms IS NULL OR activated_at_ms >= created_at_ms)
);

CREATE UNIQUE INDEX idx_routing_health_one_active_generation
    ON routing_health_generations(status) WHERE status = 'active';

INSERT INTO routing_health_generations (
    generation_id, projector_version, status, projected_row_count,
    projected_content_hash, created_at_ms, activated_at_ms
) VALUES (
    'scoped-health-bootstrap-v1', 'scoped-health-projector-v1', 'active', 0,
    'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855', 0, 0
);

CREATE TABLE routing_health_observations (
    ingestion_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id TEXT NOT NULL UNIQUE CHECK (length(observation_id) BETWEEN 1 AND 160),
    producer_id TEXT NOT NULL CHECK (length(producer_id) BETWEEN 1 AND 96),
    producer_sequence INTEGER NOT NULL CHECK (producer_sequence >= 0),
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    logical_request_id TEXT NOT NULL CHECK (length(logical_request_id) BETWEEN 1 AND 160),
    attempt_ordinal INTEGER NOT NULL CHECK (attempt_ordinal BETWEEN 0 AND 63),
    terminal_kind TEXT NOT NULL CHECK (length(terminal_kind) BETWEEN 1 AND 64),
    ingested_at_ms INTEGER NOT NULL CHECK (ingested_at_ms >= 0),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 96),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'station_key_credential', 'station_account', 'station_group',
        'station_endpoint', 'model_on_key'
    )),
    failure_dimension TEXT NOT NULL CHECK (failure_dimension IN (
        'credential', 'account_lifecycle', 'group_subscription', 'balance',
        'quota', 'rate_limit', 'endpoint_availability'
    )),
    station_id TEXT NOT NULL CHECK (length(station_id) BETWEEN 1 AND 160),
    station_key_id TEXT CHECK (station_key_id IS NULL OR length(station_key_id) BETWEEN 1 AND 160),
    group_binding_id TEXT CHECK (group_binding_id IS NULL OR length(group_binding_id) BETWEEN 1 AND 160),
    resolved_model_commitment TEXT CHECK (resolved_model_commitment IS NULL OR length(resolved_model_commitment) = 64),
    credential_revision INTEGER CHECK (credential_revision IS NULL OR credential_revision > 0),
    account_revision INTEGER CHECK (account_revision IS NULL OR account_revision > 0),
    group_revision INTEGER CHECK (group_revision IS NULL OR group_revision > 0),
    endpoint_revision INTEGER CHECK (endpoint_revision IS NULL OR endpoint_revision > 0),
    model_alias_revision INTEGER CHECK (model_alias_revision IS NULL OR model_alias_revision > 0),
    verdict TEXT CHECK (verdict IS NULL OR verdict IN ('degraded', 'cooldown', 'blocked')),
    cooldown_until_ms INTEGER CHECK (cooldown_until_ms IS NULL OR cooldown_until_ms >= 0),
    evidence_code TEXT NOT NULL CHECK (length(evidence_code) BETWEEN 1 AND 96),
    projector_profile_version TEXT NOT NULL CHECK (length(projector_profile_version) BETWEEN 1 AND 96),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (producer_id, producer_sequence),
    UNIQUE (logical_request_id, attempt_ordinal, terminal_kind, scope, failure_dimension),
    CHECK ((verdict = 'cooldown' AND cooldown_until_ms IS NOT NULL)
        OR (verdict IS NOT 'cooldown' AND cooldown_until_ms IS NULL)),
    CHECK (
        (scope_kind = 'station_key_credential' AND station_key_id IS NOT NULL
            AND credential_revision IS NOT NULL AND account_revision IS NULL
            AND group_binding_id IS NULL AND group_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_account' AND station_key_id IS NULL
            AND account_revision IS NOT NULL AND credential_revision IS NULL
            AND group_binding_id IS NULL AND group_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_group' AND station_key_id IS NULL
            AND group_binding_id IS NOT NULL AND group_revision IS NOT NULL
            AND credential_revision IS NULL AND account_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_endpoint' AND station_key_id IS NULL
            AND endpoint_revision IS NOT NULL AND credential_revision IS NULL
            AND account_revision IS NULL AND group_binding_id IS NULL
            AND group_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'model_on_key' AND station_key_id IS NOT NULL
            AND resolved_model_commitment IS NOT NULL AND credential_revision IS NOT NULL
            AND endpoint_revision IS NOT NULL AND model_alias_revision IS NOT NULL
            AND account_revision IS NULL AND group_binding_id IS NULL
            AND group_revision IS NULL)
    )
);

CREATE INDEX idx_routing_health_observations_cursor
    ON routing_health_observations(ingestion_sequence ASC);
CREATE INDEX idx_routing_health_observations_scope_cursor
    ON routing_health_observations(scope, failure_dimension, ingestion_sequence ASC);

CREATE TABLE routing_health_verdicts (
    generation_id TEXT NOT NULL REFERENCES routing_health_generations(generation_id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 96),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'station_key_credential', 'station_account', 'station_group',
        'station_endpoint', 'model_on_key'
    )),
    failure_dimension TEXT NOT NULL CHECK (failure_dimension IN (
        'credential', 'account_lifecycle', 'group_subscription', 'balance',
        'quota', 'rate_limit', 'endpoint_availability'
    )),
    station_id TEXT NOT NULL CHECK (length(station_id) BETWEEN 1 AND 160),
    station_key_id TEXT CHECK (station_key_id IS NULL OR length(station_key_id) BETWEEN 1 AND 160),
    group_binding_id TEXT CHECK (group_binding_id IS NULL OR length(group_binding_id) BETWEEN 1 AND 160),
    resolved_model_commitment TEXT CHECK (resolved_model_commitment IS NULL OR length(resolved_model_commitment) = 64),
    credential_revision INTEGER CHECK (credential_revision IS NULL OR credential_revision > 0),
    account_revision INTEGER CHECK (account_revision IS NULL OR account_revision > 0),
    group_revision INTEGER CHECK (group_revision IS NULL OR group_revision > 0),
    endpoint_revision INTEGER CHECK (endpoint_revision IS NULL OR endpoint_revision > 0),
    model_alias_revision INTEGER CHECK (model_alias_revision IS NULL OR model_alias_revision > 0),
    verdict TEXT NOT NULL CHECK (verdict IN ('degraded', 'cooldown', 'blocked')),
    cooldown_until_ms INTEGER CHECK (cooldown_until_ms IS NULL OR cooldown_until_ms >= 0),
    evidence_code TEXT NOT NULL CHECK (length(evidence_code) BETWEEN 1 AND 96),
    source_observation_id TEXT NOT NULL CHECK (length(source_observation_id) BETWEEN 1 AND 160),
    source_ingested_at_ms INTEGER NOT NULL CHECK (source_ingested_at_ms >= 0),
    source_ingestion_sequence INTEGER NOT NULL CHECK (source_ingestion_sequence > 0),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (generation_id, scope, failure_dimension),
    CHECK ((verdict = 'cooldown' AND cooldown_until_ms IS NOT NULL)
        OR (verdict != 'cooldown' AND cooldown_until_ms IS NULL)),
    CHECK (
        (scope_kind = 'station_key_credential' AND station_key_id IS NOT NULL
            AND credential_revision IS NOT NULL AND account_revision IS NULL
            AND group_binding_id IS NULL AND group_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_account' AND station_key_id IS NULL
            AND account_revision IS NOT NULL AND credential_revision IS NULL
            AND group_binding_id IS NULL AND group_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_group' AND station_key_id IS NULL
            AND group_binding_id IS NOT NULL AND group_revision IS NOT NULL
            AND credential_revision IS NULL AND account_revision IS NULL
            AND endpoint_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'station_endpoint' AND station_key_id IS NULL
            AND endpoint_revision IS NOT NULL AND credential_revision IS NULL
            AND account_revision IS NULL AND group_binding_id IS NULL
            AND group_revision IS NULL AND resolved_model_commitment IS NULL
            AND model_alias_revision IS NULL)
        OR (scope_kind = 'model_on_key' AND station_key_id IS NOT NULL
            AND resolved_model_commitment IS NOT NULL AND credential_revision IS NOT NULL
            AND endpoint_revision IS NOT NULL AND model_alias_revision IS NOT NULL
            AND account_revision IS NULL AND group_binding_id IS NULL
            AND group_revision IS NULL)
    )
);

CREATE INDEX idx_routing_health_verdicts_planner
    ON routing_health_verdicts(generation_id, scope_kind, station_id, station_key_id, failure_dimension);

CREATE TABLE routing_health_projector_state (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    active_generation_id TEXT NOT NULL REFERENCES routing_health_generations(generation_id),
    watermark_ingested_at_ms INTEGER CHECK (watermark_ingested_at_ms IS NULL OR watermark_ingested_at_ms >= 0),
    watermark_ingestion_sequence INTEGER CHECK (watermark_ingestion_sequence IS NULL OR watermark_ingestion_sequence > 0),
    watermark_observation_id TEXT CHECK (watermark_observation_id IS NULL OR length(watermark_observation_id) BETWEEN 1 AND 160),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

INSERT INTO routing_health_projector_state (
    singleton_key, projector_version, active_generation_id, updated_at_ms
) VALUES (1, 'scoped-health-projector-v1', 'scoped-health-bootstrap-v1', 0);

-- Unsupported-model evidence has one durable owner. It is deliberately not
-- duplicated as a model_on_key health verdict.
CREATE TABLE routing_capability_model_observations (
    ingestion_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id TEXT NOT NULL UNIQUE CHECK (length(observation_id) BETWEEN 1 AND 160),
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    logical_request_id TEXT NOT NULL CHECK (length(logical_request_id) BETWEEN 1 AND 160),
    attempt_ordinal INTEGER NOT NULL CHECK (attempt_ordinal BETWEEN 0 AND 63),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    resolved_model TEXT NOT NULL CHECK (length(resolved_model) BETWEEN 1 AND 256),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    model_alias_revision INTEGER NOT NULL CHECK (model_alias_revision > 0),
    verdict TEXT NOT NULL CHECK (verdict = 'unsupported'),
    evidence_code TEXT NOT NULL CHECK (length(evidence_code) BETWEEN 1 AND 96),
    classifier_profile_version TEXT NOT NULL CHECK (length(classifier_profile_version) BETWEEN 1 AND 96),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (logical_request_id, attempt_ordinal, station_key_id, resolved_model)
);

CREATE TABLE routing_capability_model_verdicts (
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    resolved_model TEXT NOT NULL CHECK (length(resolved_model) BETWEEN 1 AND 256),
    credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
    endpoint_revision INTEGER NOT NULL CHECK (endpoint_revision > 0),
    model_alias_revision INTEGER NOT NULL CHECK (model_alias_revision > 0),
    verdict TEXT NOT NULL CHECK (verdict = 'unsupported'),
    source_observation_id TEXT NOT NULL CHECK (length(source_observation_id) BETWEEN 1 AND 160),
    source_ingestion_sequence INTEGER NOT NULL CHECK (source_ingestion_sequence > 0),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (station_key_id, resolved_model, credential_revision, endpoint_revision, model_alias_revision)
);

CREATE INDEX idx_routing_capability_model_planner
    ON routing_capability_model_verdicts(station_key_id, credential_revision, endpoint_revision, model_alias_revision, resolved_model);

-- Revisions are fences, not TTLs. These baselines let typed account/group
-- scopes recover only when their subject changes or explicit recovery arrives.
INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'station_account:' || id, 1, 0, 'baseline_snapshot' FROM stations
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'station_group:' || id, 1, 0, 'baseline_snapshot' FROM station_group_bindings
WHERE 1
ON CONFLICT(scope) DO NOTHING;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
VALUES ('model_alias:direct', 1, 0, 'baseline_snapshot')
ON CONFLICT(scope) DO NOTHING;

CREATE TRIGGER routing_health_station_account_revision
AFTER UPDATE ON station_credentials
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('station_account:' || NEW.station_id, 1, 0, 'transactional_write')
    ON CONFLICT(scope) DO UPDATE SET revision = revision + 1, provenance = 'transactional_write';
END;

CREATE TRIGGER routing_health_station_group_revision
AFTER UPDATE ON station_group_bindings
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('station_group:' || NEW.id, 1, 0, 'transactional_write')
    ON CONFLICT(scope) DO UPDATE SET revision = revision + 1, provenance = 'transactional_write';
END;

CREATE TRIGGER routing_health_station_group_revision_insert
AFTER INSERT ON station_group_bindings
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('station_group:' || NEW.id, 1, 0, 'transactional_write')
    ON CONFLICT(scope) DO NOTHING;
END;

CREATE TRIGGER routing_health_credential_revision
AFTER UPDATE OF api_key, api_key_secret_id ON station_keys
WHEN OLD.api_key IS NOT NEW.api_key OR OLD.api_key_secret_id IS NOT NEW.api_key_secret_id
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('station_key:' || NEW.id, 1, 0, 'transactional_write')
    ON CONFLICT(scope) DO UPDATE SET revision = revision + 1, provenance = 'transactional_write';
END;

CREATE TRIGGER routing_health_model_alias_revision
AFTER UPDATE OF client_model, upstream_model, enabled ON model_aliases
WHEN OLD.client_model IS NOT NEW.client_model
  OR OLD.upstream_model IS NOT NEW.upstream_model
  OR OLD.enabled IS NOT NEW.enabled
BEGIN
    INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
    VALUES ('model_alias:' || NEW.id, 1, 0, 'transactional_write')
    ON CONFLICT(scope) DO UPDATE SET revision = revision + 1, provenance = 'transactional_write';
END;

UPDATE persistence_schema_compatibility
SET schema_version = 35,
    updated_by_migration = 35,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version = 34;

CREATE TEMP TABLE persistence_v35_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 35)
);
INSERT INTO persistence_v35_schema_guard (schema_version)
SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1;
DROP TABLE persistence_v35_schema_guard;
