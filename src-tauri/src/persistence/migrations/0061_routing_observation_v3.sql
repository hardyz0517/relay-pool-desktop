-- Routing v3 observation and attempt ledger.
--
-- This migration is additive. Existing routing_observations rows are kept as
-- legacy evidence and are never re-clustered during schema upgrade. New v3
-- producers must populate the structured columns; the v3 write path owns
-- ingestion_sequence allocation through routing_v3_ingestion_sequence.

ALTER TABLE routing_observations ADD COLUMN ingestion_sequence INTEGER;
ALTER TABLE routing_observations ADD COLUMN event_id TEXT;
ALTER TABLE routing_observations ADD COLUMN attempt_id TEXT;
ALTER TABLE routing_observations ADD COLUMN correlation_id TEXT;
ALTER TABLE routing_observations ADD COLUMN station_key_id TEXT;
ALTER TABLE routing_observations ADD COLUMN station_key_lifecycle_revision INTEGER;
ALTER TABLE routing_observations ADD COLUMN attempt_index INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routing_observations ADD COLUMN candidate_admitted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routing_observations ADD COLUMN candidate_admitted_at_ms INTEGER;
ALTER TABLE routing_observations ADD COLUMN capacity_lease_id TEXT;
ALTER TABLE routing_observations ADD COLUMN half_open_lease_id TEXT;
ALTER TABLE routing_observations ADD COLUMN boundary_crossed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE routing_observations ADD COLUMN response_origin TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE routing_observations ADD COLUMN event_time_status TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE routing_observations ADD COLUMN outcome TEXT;
ALTER TABLE routing_observations ADD COLUMN failure_code TEXT;
ALTER TABLE routing_observations ADD COLUMN failure_attribution TEXT;
ALTER TABLE routing_observations ADD COLUMN ttft_ms INTEGER;
ALTER TABLE routing_observations ADD COLUMN observed_at_ms INTEGER;
ALTER TABLE routing_observations ADD COLUMN comparability_key TEXT;
ALTER TABLE routing_observations ADD COLUMN recovery_origin TEXT;
ALTER TABLE routing_observations ADD COLUMN retry_disposition TEXT;
ALTER TABLE routing_observations ADD COLUMN algorithm_version TEXT;
ALTER TABLE routing_observations ADD COLUMN source_weight_revision INTEGER;
ALTER TABLE routing_observations ADD COLUMN quality_policy_revision INTEGER;
ALTER TABLE routing_observations ADD COLUMN generation_eligibility TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE routing_observations ADD COLUMN cluster_finalized INTEGER NOT NULL DEFAULT 1;
ALTER TABLE routing_observations ADD COLUMN cluster_expected_attempt_count INTEGER NOT NULL DEFAULT 1;
ALTER TABLE routing_observations ADD COLUMN cluster_finalized_at_ms INTEGER;
ALTER TABLE routing_observations ADD COLUMN cluster_finalization_reason TEXT;

-- Give legacy rows a stable receive order for cursor compatibility. This is
-- an audit ordering only; it is not used to infer event time or a quality
-- sample. New rows receive a value from the shared allocator below.
UPDATE routing_observations
SET ingestion_sequence = (
    SELECT ordered.sequence_number
    FROM (
        SELECT id, ROW_NUMBER() OVER (ORDER BY ingested_at_ms ASC, id ASC) AS sequence_number
        FROM routing_observations
    ) AS ordered
    WHERE ordered.id = routing_observations.id
)
WHERE ingestion_sequence IS NULL;

CREATE UNIQUE INDEX idx_routing_observations_v3_ingestion_sequence
    ON routing_observations(ingestion_sequence)
    WHERE ingestion_sequence IS NOT NULL;

CREATE UNIQUE INDEX idx_routing_observations_v3_event_id
    ON routing_observations(event_id)
    WHERE event_id IS NOT NULL;

CREATE UNIQUE INDEX idx_routing_observations_v3_attempt_id
    ON routing_observations(attempt_id)
    WHERE attempt_id IS NOT NULL;

CREATE UNIQUE INDEX idx_routing_observations_v3_attempt_identity
    ON routing_observations(
        source, station_key_id, station_key_lifecycle_revision,
        correlation_id, attempt_index
    )
    WHERE generation_eligibility IN ('active', 'next');

CREATE INDEX idx_routing_observations_v3_key_event_time
    ON routing_observations(
        station_key_id, station_key_lifecycle_revision, event_at_ms, id
    );

CREATE INDEX idx_routing_observations_v3_source_cursor
    ON routing_observations(
        source, station_key_id, station_key_lifecycle_revision,
        ingestion_sequence, id
    );

CREATE TABLE routing_v3_ingestion_sequence (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    next_sequence INTEGER NOT NULL CHECK (next_sequence > 0)
);

INSERT INTO routing_v3_ingestion_sequence (singleton_key, next_sequence)
VALUES (
    1,
    COALESCE((SELECT MAX(ingestion_sequence) + 1 FROM routing_observations), 1)
);

-- The trigger is a compatibility bridge for old append callers. The v3
-- observation writer should reserve the sequence in its own transaction and
-- provide it explicitly; the trigger only fills an omitted value.
CREATE TRIGGER routing_observations_v3_assign_ingestion_sequence
AFTER INSERT ON routing_observations
WHEN NEW.ingestion_sequence IS NULL
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = next_sequence + 1
    WHERE singleton_key = 1;
    UPDATE routing_observations
    SET ingestion_sequence = (
        SELECT next_sequence - 1
        FROM routing_v3_ingestion_sequence
        WHERE singleton_key = 1
    )
    WHERE id = NEW.id;
END;

CREATE TRIGGER routing_observations_v3_advance_ingestion_sequence
AFTER INSERT ON routing_observations
WHEN NEW.ingestion_sequence IS NOT NULL
 AND NEW.ingestion_sequence >= (SELECT next_sequence FROM routing_v3_ingestion_sequence WHERE singleton_key = 1)
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = NEW.ingestion_sequence + 1
    WHERE singleton_key = 1;
END;

-- Structured v3 rows are immutable facts. The receive sequence is deliberately
-- excluded so the compatibility trigger above can fill it after INSERT.
CREATE TRIGGER routing_observations_v3_immutable
BEFORE UPDATE OF
    event_id, attempt_id, correlation_id, station_key_id,
    station_key_lifecycle_revision, attempt_index, candidate_admitted,
    candidate_admitted_at_ms, capacity_lease_id, half_open_lease_id,
    boundary_crossed, response_origin, event_time_status, outcome,
    failure_code, failure_attribution, ttft_ms, observed_at_ms,
    comparability_key, recovery_origin, retry_disposition, algorithm_version,
    source_weight_revision, quality_policy_revision, generation_eligibility,
    cluster_finalized, cluster_expected_attempt_count,
    cluster_finalized_at_ms, cluster_finalization_reason
ON routing_observations
WHEN OLD.generation_eligibility IN ('active', 'next')
 AND (
    OLD.event_id IS NOT NEW.event_id
    OR OLD.attempt_id IS NOT NEW.attempt_id
    OR OLD.correlation_id IS NOT NEW.correlation_id
    OR OLD.station_key_id IS NOT NEW.station_key_id
    OR OLD.station_key_lifecycle_revision IS NOT NEW.station_key_lifecycle_revision
    OR OLD.attempt_index IS NOT NEW.attempt_index
    OR OLD.candidate_admitted IS NOT NEW.candidate_admitted
    OR OLD.candidate_admitted_at_ms IS NOT NEW.candidate_admitted_at_ms
    OR OLD.capacity_lease_id IS NOT NEW.capacity_lease_id
    OR OLD.half_open_lease_id IS NOT NEW.half_open_lease_id
    OR OLD.boundary_crossed IS NOT NEW.boundary_crossed
    OR OLD.response_origin IS NOT NEW.response_origin
    OR OLD.event_time_status IS NOT NEW.event_time_status
    OR OLD.outcome IS NOT NEW.outcome
    OR OLD.failure_code IS NOT NEW.failure_code
    OR OLD.failure_attribution IS NOT NEW.failure_attribution
    OR OLD.ttft_ms IS NOT NEW.ttft_ms
    OR OLD.observed_at_ms IS NOT NEW.observed_at_ms
    OR OLD.comparability_key IS NOT NEW.comparability_key
    OR OLD.recovery_origin IS NOT NEW.recovery_origin
    OR OLD.retry_disposition IS NOT NEW.retry_disposition
    OR OLD.algorithm_version IS NOT NEW.algorithm_version
    OR OLD.source_weight_revision IS NOT NEW.source_weight_revision
    OR OLD.quality_policy_revision IS NOT NEW.quality_policy_revision
    OR OLD.generation_eligibility IS NOT NEW.generation_eligibility
    OR OLD.cluster_finalized IS NOT NEW.cluster_finalized
    OR OLD.cluster_expected_attempt_count IS NOT NEW.cluster_expected_attempt_count
    OR OLD.cluster_finalized_at_ms IS NOT NEW.cluster_finalized_at_ms
    OR OLD.cluster_finalization_reason IS NOT NEW.cluster_finalization_reason
 )
BEGIN
    SELECT RAISE(ABORT, 'v3 routing observation is immutable');
END;

-- Conditional checks are triggers because SQLite cannot add a CHECK to an
-- already populated table without rebuilding and rewriting legacy rows.
CREATE TRIGGER routing_observations_v3_validate_insert
BEFORE INSERT ON routing_observations
WHEN NEW.generation_eligibility IN ('active', 'next')
 AND (
    NEW.source NOT IN ('real_request', 'active_probe', 'administrative')
    OR NEW.attempt_index < 0 OR NEW.attempt_index > 1023
    OR NEW.candidate_admitted NOT IN (0, 1)
    OR (NEW.candidate_admitted = 1 AND NEW.candidate_admitted_at_ms IS NULL)
    OR NEW.boundary_crossed NOT IN (0, 1)
    OR NEW.response_origin IS NULL
    OR NEW.response_origin NOT IN ('upstream', 'relay', 'unknown')
    OR NEW.event_time_status IS NULL
    OR NEW.event_time_status NOT IN ('valid', 'missing', 'invalid')
    OR NEW.outcome IS NULL
    OR NEW.outcome NOT IN ('success', 'attributable_failure', 'excluded')
    OR NEW.failure_attribution IS NULL
    OR NEW.failure_attribution NOT IN ('key', 'local', 'client', 'unknown')
    OR NEW.retry_disposition IS NULL
    OR NEW.retry_disposition NOT IN ('end', 'retryable_before_commit', 'stop_request')
    OR NEW.generation_eligibility NOT IN ('active', 'next')
    OR NEW.algorithm_version IS NULL
    OR length(NEW.algorithm_version) NOT BETWEEN 1 AND 96
    OR NEW.source_weight_revision IS NULL OR NEW.source_weight_revision <= 0
    OR NEW.quality_policy_revision IS NULL OR NEW.quality_policy_revision <= 0
    OR NEW.cluster_expected_attempt_count < 0
    OR NEW.cluster_expected_attempt_count > 1023
    OR NEW.cluster_finalized NOT IN (0, 1)
    OR (NEW.cluster_finalized = 1 AND (
        NEW.cluster_finalized_at_ms IS NULL OR NEW.cluster_finalization_reason IS NULL
    ))
    OR (NEW.cluster_finalized = 0 AND (
        NEW.cluster_finalized_at_ms IS NOT NULL OR NEW.cluster_finalization_reason IS NOT NULL
    ))
    OR (NEW.outcome = 'excluded' AND NOT (
        NEW.failure_attribution IN ('local', 'client')
        AND NEW.response_origin = 'relay'
    ))
    OR (NEW.outcome = 'attributable_failure' AND NEW.boundary_crossed <> 1)
    OR (NEW.outcome = 'success' AND NOT (
        NEW.response_origin = 'upstream' AND NEW.boundary_crossed = 1
    ))
    OR (NEW.source IN ('real_request', 'active_probe') AND NOT (
        NEW.event_id IS NOT NULL AND length(NEW.event_id) BETWEEN 1 AND 160
        AND NEW.attempt_id IS NOT NULL AND length(NEW.attempt_id) BETWEEN 1 AND 160
        AND NEW.correlation_id IS NOT NULL AND length(NEW.correlation_id) BETWEEN 1 AND 192
        AND NEW.station_key_id IS NOT NULL AND length(NEW.station_key_id) BETWEEN 1 AND 160
        AND NEW.station_key_lifecycle_revision IS NOT NULL
        AND NEW.station_key_lifecycle_revision > 0
    ))
    OR (NEW.source = 'administrative' AND NOT (
        NEW.station_key_id IS NULL
        AND NEW.station_key_lifecycle_revision IS NULL
        AND NEW.correlation_id IS NULL
        AND NEW.attempt_id IS NULL
        AND NEW.event_id IS NULL
        AND NEW.attempt_index = 0
        AND NEW.boundary_crossed = 0
        AND NEW.outcome = 'excluded'
        AND NEW.failure_attribution = 'local'
    ))
 )
BEGIN
    SELECT RAISE(ABORT, 'invalid v3 routing observation identity or outcome');
END;

CREATE TABLE routing_attempt_v3 (
    attempt_id TEXT PRIMARY KEY CHECK (length(attempt_id) BETWEEN 1 AND 160),
    event_id TEXT NOT NULL UNIQUE CHECK (length(event_id) BETWEEN 1 AND 160),
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) BETWEEN 1 AND 192),
    source TEXT NOT NULL CHECK (source IN ('real_request', 'active_probe')),
    station_key_id TEXT CHECK (station_key_id IS NULL OR length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER CHECK (station_key_lifecycle_revision IS NULL OR station_key_lifecycle_revision > 0),
    attempt_index INTEGER NOT NULL CHECK (attempt_index BETWEEN 0 AND 1023),
    candidate_admitted INTEGER NOT NULL CHECK (candidate_admitted IN (0, 1)),
    candidate_admitted_at_ms INTEGER,
    capacity_lease_id TEXT,
    half_open_lease_id TEXT,
    lease_revision INTEGER CHECK (lease_revision IS NULL OR lease_revision > 0),
    deadline_at_ms INTEGER CHECK (deadline_at_ms IS NULL OR deadline_at_ms >= 0),
    boundary_crossed INTEGER NOT NULL DEFAULT 0 CHECK (boundary_crossed IN (0, 1)),
    boundary_crossed_at_ms INTEGER CHECK (boundary_crossed_at_ms IS NULL OR boundary_crossed_at_ms >= 0),
    response_origin TEXT NOT NULL DEFAULT 'unknown' CHECK (response_origin IN ('upstream', 'relay', 'unknown')),
    event_time_status TEXT NOT NULL DEFAULT 'valid' CHECK (event_time_status IN ('valid', 'missing', 'invalid')),
    outcome TEXT CHECK (outcome IS NULL OR outcome IN ('success', 'attributable_failure', 'excluded')),
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96),
    failure_attribution TEXT CHECK (failure_attribution IS NULL OR failure_attribution IN ('key', 'local', 'client', 'unknown')),
    latency_ms INTEGER CHECK (latency_ms IS NULL OR latency_ms >= 0),
    ttft_ms INTEGER CHECK (ttft_ms IS NULL OR ttft_ms >= 0),
    event_at_ms INTEGER CHECK (event_at_ms IS NULL OR event_at_ms >= 0),
    observed_at_ms INTEGER CHECK (observed_at_ms IS NULL OR observed_at_ms >= 0),
    ingested_at_ms INTEGER CHECK (ingested_at_ms IS NULL OR ingested_at_ms >= 0),
    ingestion_sequence INTEGER,
    comparability_key TEXT CHECK (comparability_key IS NULL OR length(comparability_key) BETWEEN 1 AND 192),
    recovery_origin TEXT CHECK (recovery_origin IS NULL OR recovery_origin IN ('normal', 'crash_recovery', 'lease_reaper')),
    retry_disposition TEXT CHECK (retry_disposition IS NULL OR retry_disposition IN ('end', 'retryable_before_commit', 'stop_request')),
    algorithm_version TEXT CHECK (algorithm_version IS NULL OR length(algorithm_version) BETWEEN 1 AND 96),
    source_weight_revision INTEGER CHECK (source_weight_revision IS NULL OR source_weight_revision > 0),
    quality_policy_revision INTEGER CHECK (quality_policy_revision IS NULL OR quality_policy_revision > 0),
    generation_eligibility TEXT NOT NULL DEFAULT 'active' CHECK (generation_eligibility IN ('active', 'next', 'legacy')),
    terminal_state TEXT NOT NULL DEFAULT 'pending' CHECK (
        terminal_state IN ('pending', 'success', 'attributable_failure', 'excluded', 'local_abandoned', 'upstream_uncertain')
    ),
    terminal_at_ms INTEGER CHECK (terminal_at_ms IS NULL OR terminal_at_ms >= 0),
    released_at_ms INTEGER CHECK (released_at_ms IS NULL OR released_at_ms >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    UNIQUE (source, station_key_id, station_key_lifecycle_revision, correlation_id, attempt_index),
    CHECK ((candidate_admitted = 1 AND candidate_admitted_at_ms IS NOT NULL)
        OR candidate_admitted = 0),
    CHECK ((terminal_state = 'pending' AND outcome IS NULL AND terminal_at_ms IS NULL)
        OR (terminal_state <> 'pending' AND outcome IS NOT NULL
            AND failure_attribution IS NOT NULL AND terminal_at_ms IS NOT NULL)),
    CHECK ((boundary_crossed = 1 AND boundary_crossed_at_ms IS NOT NULL)
        OR boundary_crossed = 0),
    CHECK (terminal_state <> 'local_abandoned' OR (
        outcome = 'excluded' AND failure_attribution = 'local'
        AND response_origin = 'relay' AND boundary_crossed = 0
    )),
    CHECK (terminal_state <> 'upstream_uncertain' OR (
        outcome = 'attributable_failure' AND failure_attribution = 'key'
        AND response_origin = 'unknown' AND boundary_crossed = 1
    )),
    CHECK (terminal_state <> 'success' OR (
        outcome = 'success' AND failure_attribution = 'key'
        AND response_origin = 'upstream' AND boundary_crossed = 1
    )),
    CHECK (terminal_state <> 'excluded' OR outcome = 'excluded')
);

CREATE INDEX idx_routing_attempt_v3_key_cursor
    ON routing_attempt_v3(
        station_key_id, station_key_lifecycle_revision,
        ingestion_sequence, attempt_id
    );
CREATE INDEX idx_routing_attempt_v3_correlation
    ON routing_attempt_v3(source, station_key_id, station_key_lifecycle_revision, correlation_id, attempt_index);

-- Attempts and observations share one monotonic ingestion cursor. The
-- attempt ledger has no JSON fallback or process-local counter; allocating in
-- SQLite keeps ordering stable across restarts and makes replay deterministic.
CREATE TRIGGER routing_attempt_v3_assign_ingestion_sequence
AFTER INSERT ON routing_attempt_v3
WHEN NEW.ingestion_sequence IS NULL
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = next_sequence + 1
    WHERE singleton_key = 1;
    UPDATE routing_attempt_v3
    SET ingestion_sequence = (
        SELECT next_sequence - 1
        FROM routing_v3_ingestion_sequence
        WHERE singleton_key = 1
    )
    WHERE attempt_id = NEW.attempt_id;
END;

CREATE TRIGGER routing_attempt_v3_advance_ingestion_sequence
AFTER INSERT ON routing_attempt_v3
WHEN NEW.ingestion_sequence IS NOT NULL
 AND NEW.ingestion_sequence >= (SELECT next_sequence FROM routing_v3_ingestion_sequence WHERE singleton_key = 1)
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = NEW.ingestion_sequence + 1
    WHERE singleton_key = 1;
END;

CREATE TABLE routing_attempt_cluster_v3 (
    source TEXT NOT NULL CHECK (source IN ('real_request', 'active_probe')),
    station_key_id TEXT CHECK (station_key_id IS NULL OR length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER CHECK (station_key_lifecycle_revision IS NULL OR station_key_lifecycle_revision > 0),
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) BETWEEN 1 AND 192),
    expected_attempt_count INTEGER NOT NULL CHECK (expected_attempt_count BETWEEN 0 AND 1023),
    cluster_finalized INTEGER NOT NULL DEFAULT 0 CHECK (cluster_finalized IN (0, 1)),
    cluster_finalized_at_ms INTEGER CHECK (cluster_finalized_at_ms IS NULL OR cluster_finalized_at_ms >= 0),
    cluster_finalization_reason TEXT CHECK (cluster_finalization_reason IS NULL OR length(cluster_finalization_reason) BETWEEN 1 AND 96),
    generation_eligibility TEXT NOT NULL DEFAULT 'active' CHECK (generation_eligibility IN ('active', 'next', 'legacy')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    PRIMARY KEY (source, station_key_id, station_key_lifecycle_revision, correlation_id),
    CHECK ((cluster_finalized = 1 AND cluster_finalized_at_ms IS NOT NULL AND cluster_finalization_reason IS NOT NULL)
        OR (cluster_finalized = 0 AND cluster_finalized_at_ms IS NULL AND cluster_finalization_reason IS NULL)),
    CHECK (expected_attempt_count <> 0 OR (
        station_key_id IS NULL AND station_key_lifecycle_revision IS NULL
        AND cluster_finalized = 1 AND cluster_finalization_reason = 'no_attempts'
    )),
    CHECK (expected_attempt_count = 0 OR (
        station_key_id IS NOT NULL AND station_key_lifecycle_revision IS NOT NULL
    ))
);

CREATE UNIQUE INDEX idx_routing_attempt_cluster_v3_no_attempts
    ON routing_attempt_cluster_v3(source, correlation_id)
    WHERE station_key_id IS NULL;

CREATE INDEX idx_routing_attempt_cluster_v3_pending
    ON routing_attempt_cluster_v3(
        station_key_id, station_key_lifecycle_revision, cluster_finalized, updated_at_ms
    );

CREATE TABLE routing_quality_generation_v3 (
    quality_generation_id TEXT PRIMARY KEY CHECK (length(quality_generation_id) BETWEEN 5 AND 192),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    quality_policy_revision INTEGER NOT NULL CHECK (quality_policy_revision > 0),
    quality_algorithm_version TEXT NOT NULL CHECK (length(quality_algorithm_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN ('building', 'ready', 'active', 'retired', 'failed')),
    evaluation_at_ms INTEGER CHECK (evaluation_at_ms IS NULL OR evaluation_at_ms >= 0),
    input_observation_watermark INTEGER CHECK (input_observation_watermark IS NULL OR input_observation_watermark >= 0),
    input_observation_hash TEXT CHECK (input_observation_hash IS NULL OR length(input_observation_hash) = 64),
    output_content_hash TEXT CHECK (output_content_hash IS NULL OR length(output_content_hash) = 64),
    checkpoint_ref TEXT CHECK (checkpoint_ref IS NULL OR length(checkpoint_ref) BETWEEN 1 AND 192),
    processed_observation_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_observation_count >= 0),
    cursor_station_key_id TEXT,
    cursor_observation_id TEXT,
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    ready_at_ms INTEGER CHECK (ready_at_ms IS NULL OR ready_at_ms >= created_at_ms),
    activated_at_ms INTEGER CHECK (activated_at_ms IS NULL OR activated_at_ms >= created_at_ms),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    CHECK (status IN ('building', 'failed') OR (
        evaluation_at_ms IS NOT NULL
        AND input_observation_watermark IS NOT NULL
        AND input_observation_hash IS NOT NULL
        AND output_content_hash IS NOT NULL
        AND checkpoint_ref IS NOT NULL
    )),
    CHECK ((status = 'ready' AND ready_at_ms IS NOT NULL) OR status <> 'ready'),
    CHECK (status <> 'failed' OR failure_code IS NOT NULL)
);

CREATE TRIGGER routing_quality_generation_v3_immutable_identity
BEFORE UPDATE OF quality_generation_id, scope, quality_policy_revision,
    quality_algorithm_version, evaluation_at_ms, input_observation_watermark,
    input_observation_hash, output_content_hash
ON routing_quality_generation_v3
WHEN OLD.quality_generation_id IS NOT NEW.quality_generation_id
 OR OLD.scope IS NOT NEW.scope
 OR OLD.quality_policy_revision IS NOT NEW.quality_policy_revision
 OR OLD.quality_algorithm_version IS NOT NEW.quality_algorithm_version
 OR OLD.evaluation_at_ms IS NOT NEW.evaluation_at_ms
 OR OLD.input_observation_watermark IS NOT NEW.input_observation_watermark
 OR OLD.input_observation_hash IS NOT NEW.input_observation_hash
 OR OLD.output_content_hash IS NOT NEW.output_content_hash
BEGIN
    SELECT RAISE(ABORT, 'quality generation identity is immutable');
END;

CREATE TABLE routing_quality_generation_v3_checkpoint (
    quality_generation_id TEXT PRIMARY KEY REFERENCES routing_quality_generation_v3(quality_generation_id) ON DELETE CASCADE,
    input_observation_watermark INTEGER CHECK (input_observation_watermark IS NULL OR input_observation_watermark >= 0),
    cursor_station_key_id TEXT,
    cursor_observation_id TEXT,
    processed_observation_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_observation_count >= 0),
    status TEXT NOT NULL CHECK (status IN ('building', 'ready', 'failed')),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

-- Every rebuild writes into its own immutable generation namespace.  The
-- legacy scope-keyed tables remain the pre-cutover compatibility read model;
-- building, ready, active and rollback generations never overwrite them or
-- each other.
CREATE TABLE routing_quality_summary_v3 (
    quality_generation_id TEXT NOT NULL
        REFERENCES routing_quality_generation_v3(quality_generation_id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    quality_revision INTEGER NOT NULL CHECK (quality_revision > 0),
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (quality_generation_id, scope, station_key_lifecycle_revision),
    UNIQUE (
        quality_generation_id, station_key_id,
        station_key_lifecycle_revision
    ),
    CHECK (scope = 'station_key:' || station_key_id),
    CHECK (json_extract(summary_json, '$.scope') = scope),
    CHECK (json_extract(summary_json, '$.projector_version') = 'routing_quality_v3')
);

CREATE INDEX idx_routing_quality_summary_v3_generation_revision
    ON routing_quality_summary_v3(
        quality_generation_id, quality_revision, station_key_id,
        station_key_lifecycle_revision
    );

CREATE TABLE routing_quality_health_axis_v3 (
    quality_generation_id TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    axis TEXT NOT NULL CHECK (axis IN ('availability', 'latency', 'reliability', 'freshness')),
    health_revision INTEGER NOT NULL CHECK (health_revision > 0),
    value_basis_points INTEGER NOT NULL CHECK (value_basis_points BETWEEN 0 AND 10000),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (
        quality_generation_id, scope,
        station_key_lifecycle_revision, axis
    ),
    FOREIGN KEY (
        quality_generation_id, scope,
        station_key_lifecycle_revision
    ) REFERENCES routing_quality_summary_v3(
        quality_generation_id, scope,
        station_key_lifecycle_revision
    ) ON DELETE CASCADE,
    CHECK (scope = 'station_key:' || station_key_id)
);

CREATE TABLE routing_quality_incremental_checkpoint_v3 (
    quality_generation_id TEXT NOT NULL
        REFERENCES routing_quality_generation_v3(quality_generation_id) ON DELETE CASCADE,
    projector TEXT NOT NULL CHECK (length(projector) BETWEEN 1 AND 96),
    projector_version TEXT NOT NULL CHECK (length(projector_version) BETWEEN 1 AND 96),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    checkpoint_sequence INTEGER NOT NULL CHECK (checkpoint_sequence > 0),
    status TEXT NOT NULL CHECK (status IN ('ready', 'projecting', 'failed')),
    cursor_item_id TEXT CHECK (cursor_item_id IS NULL OR length(cursor_item_id) BETWEEN 1 AND 192),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (quality_generation_id, projector, projector_version, scope)
);

-- Pending request clusters are projection state, not immutable evidence. They
-- therefore follow the quality generation that owns their dedup decision.
CREATE TABLE routing_quality_pending_cluster_v3 (
    quality_generation_id TEXT NOT NULL
        REFERENCES routing_quality_generation_v3(quality_generation_id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('real_request', 'active_probe')),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) BETWEEN 1 AND 192),
    expected_attempt_count INTEGER NOT NULL CHECK (expected_attempt_count BETWEEN 1 AND 1023),
    observed_attempt_count INTEGER NOT NULL CHECK (
        observed_attempt_count BETWEEN 0 AND expected_attempt_count
    ),
    last_ingestion_sequence INTEGER NOT NULL CHECK (last_ingestion_sequence > 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (
        quality_generation_id, source, station_key_id,
        station_key_lifecycle_revision, correlation_id
    )
);

UPDATE persistence_schema_compatibility
SET schema_version = 61,
    updated_by_migration = 61,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 61;

CREATE TEMP TABLE persistence_v61_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 61)
);
INSERT INTO persistence_v61_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v61_schema_guard;
