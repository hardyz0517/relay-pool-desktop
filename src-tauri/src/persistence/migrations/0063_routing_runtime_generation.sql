-- Multi-generation runtime registry. The migration intentionally inserts no
-- v3 generation: pre_cutover with no active row is the valid legacy state.

CREATE TABLE routing_runtime_generation (
    runtime_generation_id TEXT PRIMARY KEY CHECK (length(runtime_generation_id) BETWEEN 5 AND 192),
    policy_generation_id TEXT NOT NULL CHECK (length(policy_generation_id) BETWEEN 1 AND 192)
        REFERENCES routing_policy_v3_staged(policy_generation_id) ON DELETE RESTRICT,
    quality_generation_id TEXT NOT NULL CHECK (length(quality_generation_id) BETWEEN 5 AND 192)
        REFERENCES routing_quality_generation_v3(quality_generation_id) ON DELETE RESTRICT,
    circuit_generation_id TEXT NOT NULL CHECK (length(circuit_generation_id) BETWEEN 5 AND 192)
        REFERENCES routing_circuit_generation_v3(circuit_generation_id) ON DELETE RESTRICT,
    policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
    quality_policy_revision INTEGER NOT NULL CHECK (quality_policy_revision > 0),
    circuit_policy_revision INTEGER NOT NULL CHECK (circuit_policy_revision > 0),
    algorithm_version TEXT NOT NULL CHECK (length(algorithm_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN (
        'building', 'ready', 'cutover_fencing', 'active', 'retired', 'failed'
    )),
    input_observation_watermark INTEGER CHECK (input_observation_watermark IS NULL OR input_observation_watermark >= 0),
    input_circuit_event_watermark INTEGER CHECK (input_circuit_event_watermark IS NULL OR input_circuit_event_watermark >= 0),
    policy_input_hash TEXT CHECK (policy_input_hash IS NULL OR length(policy_input_hash) = 64),
    quality_input_hash TEXT CHECK (quality_input_hash IS NULL OR length(quality_input_hash) = 64),
    circuit_input_hash TEXT CHECK (circuit_input_hash IS NULL OR length(circuit_input_hash) = 64),
    policy_content_hash TEXT CHECK (policy_content_hash IS NULL OR length(policy_content_hash) = 64),
    quality_content_hash TEXT CHECK (quality_content_hash IS NULL OR length(quality_content_hash) = 64),
    circuit_content_hash TEXT CHECK (circuit_content_hash IS NULL OR length(circuit_content_hash) = 64),
    checkpoint_ref TEXT CHECK (checkpoint_ref IS NULL OR length(checkpoint_ref) BETWEEN 1 AND 192),
    policy_checkpoint_ref TEXT CHECK (policy_checkpoint_ref IS NULL OR length(policy_checkpoint_ref) BETWEEN 1 AND 192),
    quality_checkpoint_ref TEXT CHECK (quality_checkpoint_ref IS NULL OR length(quality_checkpoint_ref) BETWEEN 1 AND 192),
    circuit_checkpoint_ref TEXT CHECK (circuit_checkpoint_ref IS NULL OR length(circuit_checkpoint_ref) BETWEEN 1 AND 192),
    cutover_fence_revision INTEGER CHECK (cutover_fence_revision IS NULL OR cutover_fence_revision > 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    ready_at_ms INTEGER CHECK (ready_at_ms IS NULL OR ready_at_ms >= created_at_ms),
    activated_at_ms INTEGER CHECK (activated_at_ms IS NULL OR activated_at_ms >= created_at_ms),
    retired_at_ms INTEGER CHECK (retired_at_ms IS NULL OR retired_at_ms >= created_at_ms),
    failed_at_ms INTEGER CHECK (failed_at_ms IS NULL OR failed_at_ms >= created_at_ms),
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    UNIQUE (policy_generation_id, quality_generation_id, circuit_generation_id),
    CHECK (status IN ('building', 'failed') OR (
        input_observation_watermark IS NOT NULL
        AND input_circuit_event_watermark IS NOT NULL
        AND policy_input_hash IS NOT NULL
        AND quality_input_hash IS NOT NULL
        AND circuit_input_hash IS NOT NULL
        AND policy_content_hash IS NOT NULL
        AND quality_content_hash IS NOT NULL
        AND circuit_content_hash IS NOT NULL
        AND checkpoint_ref IS NOT NULL
    )),
    CHECK ((status = 'ready' AND ready_at_ms IS NOT NULL) OR status <> 'ready'),
    CHECK ((status = 'active' AND activated_at_ms IS NOT NULL) OR status <> 'active'),
    CHECK ((status = 'retired' AND retired_at_ms IS NOT NULL) OR status <> 'retired'),
    CHECK ((status = 'failed' AND failed_at_ms IS NOT NULL AND failure_code IS NOT NULL) OR status <> 'failed')
);

CREATE UNIQUE INDEX idx_routing_runtime_generation_one_active
    ON routing_runtime_generation(status)
    WHERE status = 'active';

CREATE UNIQUE INDEX idx_routing_runtime_generation_one_fencing
    ON routing_runtime_generation(status)
    WHERE status = 'cutover_fencing';

CREATE INDEX idx_routing_runtime_generation_status
    ON routing_runtime_generation(status, created_at_ms DESC, runtime_generation_id ASC);

CREATE TRIGGER routing_runtime_generation_v3_immutable_identity
BEFORE UPDATE OF runtime_generation_id, policy_generation_id,
    quality_generation_id, circuit_generation_id, policy_revision,
    quality_policy_revision, circuit_policy_revision, algorithm_version,
    input_observation_watermark, input_circuit_event_watermark,
    policy_input_hash, quality_input_hash, circuit_input_hash,
    policy_content_hash, quality_content_hash, circuit_content_hash
ON routing_runtime_generation
WHEN OLD.runtime_generation_id IS NOT NEW.runtime_generation_id
 OR OLD.policy_generation_id IS NOT NEW.policy_generation_id
 OR OLD.quality_generation_id IS NOT NEW.quality_generation_id
 OR OLD.circuit_generation_id IS NOT NEW.circuit_generation_id
 OR OLD.policy_revision IS NOT NEW.policy_revision
 OR OLD.quality_policy_revision IS NOT NEW.quality_policy_revision
 OR OLD.circuit_policy_revision IS NOT NEW.circuit_policy_revision
 OR OLD.algorithm_version IS NOT NEW.algorithm_version
 OR OLD.input_observation_watermark IS NOT NEW.input_observation_watermark
 OR OLD.input_circuit_event_watermark IS NOT NEW.input_circuit_event_watermark
 OR OLD.policy_input_hash IS NOT NEW.policy_input_hash
 OR OLD.quality_input_hash IS NOT NEW.quality_input_hash
 OR OLD.circuit_input_hash IS NOT NEW.circuit_input_hash
 OR OLD.policy_content_hash IS NOT NEW.policy_content_hash
 OR OLD.quality_content_hash IS NOT NEW.quality_content_hash
 OR OLD.circuit_content_hash IS NOT NEW.circuit_content_hash
BEGIN
    SELECT RAISE(ABORT, 'runtime generation identity is immutable');
END;

CREATE TRIGGER routing_runtime_generation_v3_legal_status_transition
BEFORE UPDATE OF status ON routing_runtime_generation
WHEN OLD.status <> NEW.status
 AND NOT (
    (OLD.status = 'building' AND NEW.status IN ('ready', 'failed'))
    OR (OLD.status = 'ready' AND NEW.status IN ('cutover_fencing', 'failed'))
    OR (OLD.status = 'cutover_fencing' AND NEW.status IN ('ready', 'active', 'retired', 'failed'))
    OR (OLD.status = 'active' AND NEW.status = 'retired')
    OR (OLD.status = 'retired' AND NEW.status = 'cutover_fencing')
 )
BEGIN
    SELECT RAISE(ABORT, 'illegal routing runtime generation status transition');
END;

CREATE TABLE routing_runtime_cutover_marker (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    status TEXT NOT NULL CHECK (status IN ('pre_cutover', 'v3_active')),
    runtime_generation_id TEXT REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    fenced_runtime_generation_id TEXT REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    fence_revision INTEGER NOT NULL DEFAULT 0 CHECK (fence_revision >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    CHECK ((status = 'pre_cutover' AND runtime_generation_id IS NULL)
        OR (status = 'v3_active' AND runtime_generation_id IS NOT NULL))
);

INSERT INTO routing_runtime_cutover_marker (
    singleton_key, status, runtime_generation_id,
    fenced_runtime_generation_id, fence_revision, updated_at_ms
) VALUES (1, 'pre_cutover', NULL, NULL, 0, 0);

CREATE TABLE routing_generation_transition_audit (
    transition_id INTEGER PRIMARY KEY AUTOINCREMENT,
    transition_kind TEXT NOT NULL CHECK (transition_kind IN (
        'cutover_started', 'cutover_aborted', 'cutover_activated',
        'rollback_started', 'rollback_activated'
    )),
    source_runtime_generation_id TEXT
        REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    target_runtime_generation_id TEXT NOT NULL
        REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    fence_revision INTEGER NOT NULL CHECK (fence_revision > 0),
    reason_code TEXT CHECK (reason_code IS NULL OR length(reason_code) BETWEEN 1 AND 96),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

-- Activation is intentionally separate from rebuild completion. A Ready
-- generation cannot enter the fence until both the shadow comparison and the
-- required failure replay have produced immutable, content-addressed evidence.
CREATE TABLE routing_generation_qualification (
    runtime_generation_id TEXT PRIMARY KEY
        REFERENCES routing_runtime_generation(runtime_generation_id) ON DELETE RESTRICT,
    qualification_version TEXT NOT NULL
        CHECK (qualification_version = 'routing-generation-qualification-v1'),
    comparison_status TEXT NOT NULL CHECK (comparison_status = 'passed'),
    comparison_report_hash TEXT NOT NULL CHECK (length(comparison_report_hash) = 64),
    replay_status TEXT NOT NULL CHECK (replay_status = 'passed'),
    replay_report_hash TEXT NOT NULL CHECK (length(replay_report_hash) = 64),
    qualified_at_ms INTEGER NOT NULL CHECK (qualified_at_ms >= 0)
);

CREATE TRIGGER routing_generation_qualification_no_update
BEFORE UPDATE ON routing_generation_qualification
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification is immutable');
END;

CREATE TRIGGER routing_generation_qualification_no_delete
BEFORE DELETE ON routing_generation_qualification
BEGIN
    SELECT RAISE(ABORT, 'routing generation qualification is immutable');
END;

CREATE INDEX idx_routing_generation_transition_audit_target
    ON routing_generation_transition_audit(
        target_runtime_generation_id, created_at_ms, transition_id
    );

CREATE TRIGGER routing_generation_transition_audit_no_update
BEFORE UPDATE ON routing_generation_transition_audit
BEGIN
    SELECT RAISE(ABORT, 'routing generation transition audit is append-only');
END;

CREATE TRIGGER routing_generation_transition_audit_no_delete
BEFORE DELETE ON routing_generation_transition_audit
BEGIN
    SELECT RAISE(ABORT, 'routing generation transition audit is append-only');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 63,
    updated_by_migration = 63,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 63;

CREATE TEMP TABLE persistence_v63_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 63)
);
INSERT INTO persistence_v63_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v63_schema_guard;
