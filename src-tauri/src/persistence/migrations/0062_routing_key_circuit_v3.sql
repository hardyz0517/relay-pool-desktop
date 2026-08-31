-- Durable station-key circuit state, raw events and rebuild metadata.
-- ActiveProbe never writes circuit events; only real-request canonical
-- outcomes can advance this reducer.

-- Circuit time is a database-wide logical UTC watermark rather than a
-- producer timestamp or a per-Key value. Every reducer write advances this
-- singleton under the same SQLite write lock as its state transition.
CREATE TABLE routing_circuit_clock_v3 (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    watermark_ms INTEGER NOT NULL CHECK (watermark_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= watermark_ms)
);

INSERT INTO routing_circuit_clock_v3 (singleton_key, watermark_ms, updated_at_ms)
VALUES (1, 0, 0);

CREATE TABLE routing_circuit_state_v3 (
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    state TEXT NOT NULL CHECK (state IN ('closed', 'open', 'half_open')),
    state_revision INTEGER NOT NULL CHECK (state_revision > 0),
    policy_revision INTEGER NOT NULL DEFAULT 1 CHECK (policy_revision > 0),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    reopen_level INTEGER NOT NULL DEFAULT 0 CHECK (reopen_level >= 0),
    opened_at_ms INTEGER CHECK (opened_at_ms IS NULL OR opened_at_ms >= 0),
    cooldown_until_ms INTEGER CHECK (cooldown_until_ms IS NULL OR cooldown_until_ms >= 0),
    recovery_successes INTEGER NOT NULL DEFAULT 0 CHECK (recovery_successes >= 0),
    lease_id TEXT CHECK (lease_id IS NULL OR length(lease_id) BETWEEN 1 AND 160),
    lease_revision INTEGER CHECK (lease_revision IS NULL OR lease_revision > 0),
    lease_policy_revision INTEGER CHECK (lease_policy_revision IS NULL OR lease_policy_revision > 0),
    lease_recovery_success_threshold INTEGER CHECK (
        lease_recovery_success_threshold IS NULL OR lease_recovery_success_threshold > 0
    ),
    lease_recovery_wait_ms INTEGER CHECK (lease_recovery_wait_ms IS NULL OR lease_recovery_wait_ms > 0),
    lease_attempt_id TEXT CHECK (lease_attempt_id IS NULL OR length(lease_attempt_id) BETWEEN 1 AND 160),
    lease_expires_at_ms INTEGER CHECK (lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0),
    lease_deadline_at_ms INTEGER CHECK (lease_deadline_at_ms IS NULL OR lease_deadline_at_ms >= 0),
    boundary_crossed INTEGER CHECK (boundary_crossed IS NULL OR boundary_crossed IN (0, 1)),
    released_at_ms INTEGER CHECK (released_at_ms IS NULL OR released_at_ms >= 0),
    lease_terminal_state TEXT CHECK (lease_terminal_state IS NULL OR lease_terminal_state IN (
        'success', 'attributable_failure', 'excluded', 'local_abandoned', 'upstream_uncertain'
    )),
    monotonic_clock_watermark_ms INTEGER NOT NULL DEFAULT 0 CHECK (monotonic_clock_watermark_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (station_key_id, station_key_lifecycle_revision),
    CHECK (
        state = 'closed'
        AND opened_at_ms IS NULL
        AND cooldown_until_ms IS NULL
        AND lease_id IS NULL
        AND lease_revision IS NULL
        AND lease_policy_revision IS NULL
        AND lease_recovery_success_threshold IS NULL
        AND lease_recovery_wait_ms IS NULL
        AND lease_attempt_id IS NULL
        AND lease_expires_at_ms IS NULL
        AND lease_deadline_at_ms IS NULL
        AND boundary_crossed IS NULL
        AND released_at_ms IS NULL
        AND lease_terminal_state IS NULL
        AND recovery_successes = 0
        OR state = 'open'
        AND opened_at_ms IS NOT NULL
        AND cooldown_until_ms IS NOT NULL
        AND lease_id IS NULL
        AND lease_revision IS NULL
        AND lease_policy_revision IS NULL
        AND lease_recovery_success_threshold IS NULL
        AND lease_recovery_wait_ms IS NULL
        AND lease_attempt_id IS NULL
        AND lease_expires_at_ms IS NULL
        AND lease_deadline_at_ms IS NULL
        AND boundary_crossed IS NULL
        AND released_at_ms IS NULL
        AND lease_terminal_state IS NULL
        AND recovery_successes = 0
        OR state = 'half_open'
        AND (
            -- Half-Open may be idle between recovery requests. In that
            -- state there is no active lease and no boundary outcome to
            -- reap; a subsequent admission atomically fills the lease.
            lease_id IS NULL
            AND lease_revision IS NOT NULL
            AND lease_policy_revision IS NULL
            AND lease_recovery_success_threshold IS NULL
            AND lease_recovery_wait_ms IS NULL
            AND lease_attempt_id IS NULL
            AND lease_expires_at_ms IS NULL
            AND lease_deadline_at_ms IS NULL
            AND boundary_crossed IS NULL
            AND released_at_ms IS NULL
            AND lease_terminal_state IS NULL
            OR lease_id IS NOT NULL
            AND lease_revision IS NOT NULL
            AND lease_policy_revision IS NOT NULL
            AND lease_recovery_success_threshold IS NOT NULL
            AND lease_recovery_wait_ms IS NOT NULL
            AND lease_attempt_id IS NOT NULL
            AND lease_expires_at_ms IS NOT NULL
            AND lease_deadline_at_ms IS NOT NULL
            AND (released_at_ms IS NULL OR lease_terminal_state IS NOT NULL)
            AND (released_at_ms IS NULL OR boundary_crossed IS NOT NULL)
        )
    ),
    CHECK (state <> 'open' OR cooldown_until_ms >= opened_at_ms),
    CHECK (state <> 'half_open' OR lease_expires_at_ms <= lease_deadline_at_ms)
);

CREATE INDEX idx_routing_circuit_state_v3_admission
    ON routing_circuit_state_v3(state, station_key_id, station_key_lifecycle_revision, cooldown_until_ms);

CREATE TABLE routing_circuit_event_v3 (
    event_id TEXT NOT NULL CHECK (length(event_id) BETWEEN 1 AND 160),
    effect_kind TEXT NOT NULL CHECK (effect_kind IN ('observation', 'circuit', 'lease')),
    source TEXT NOT NULL CHECK (source = 'real_request'),
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) BETWEEN 1 AND 160),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    reducer_commit_sequence INTEGER NOT NULL CHECK (reducer_commit_sequence > 0),
    ingestion_sequence INTEGER,
    policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
    expected_state_revision INTEGER NOT NULL CHECK (expected_state_revision > 0),
    occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
    canonical_outcome TEXT NOT NULL CHECK (canonical_outcome IN ('success', 'attributable_failure', 'excluded')),
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96),
    recovery_origin TEXT NOT NULL DEFAULT 'normal' CHECK (recovery_origin IN ('normal', 'crash_recovery', 'lease_reaper')),
    retry_disposition TEXT NOT NULL CHECK (retry_disposition IN ('end', 'retryable_before_commit', 'stop_request')),
    lease_revision INTEGER CHECK (lease_revision IS NULL OR lease_revision > 0),
    boundary_crossed INTEGER NOT NULL CHECK (boundary_crossed IN (0, 1)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (event_id, effect_kind),
    UNIQUE (station_key_id, station_key_lifecycle_revision, reducer_commit_sequence),
    UNIQUE (effect_kind, station_key_id, station_key_lifecycle_revision, attempt_id),
    CHECK (canonical_outcome <> 'attributable_failure' OR boundary_crossed = 1),
    CHECK (canonical_outcome <> 'success' OR boundary_crossed = 1),
    CHECK (recovery_origin = 'normal' OR retry_disposition = 'stop_request')
);

CREATE INDEX idx_routing_circuit_event_v3_key_sequence
    ON routing_circuit_event_v3(
        station_key_id, station_key_lifecycle_revision,
        reducer_commit_sequence, event_id
    );

CREATE INDEX idx_routing_circuit_event_v3_ingestion_sequence
    ON routing_circuit_event_v3(ingestion_sequence)
    WHERE ingestion_sequence IS NOT NULL;

CREATE TRIGGER routing_circuit_events_v3_assign_ingestion_sequence
AFTER INSERT ON routing_circuit_event_v3
WHEN NEW.ingestion_sequence IS NULL
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = next_sequence + 1
    WHERE singleton_key = 1;
    UPDATE routing_circuit_event_v3
    SET ingestion_sequence = (
        SELECT next_sequence - 1
        FROM routing_v3_ingestion_sequence
        WHERE singleton_key = 1
    )
    WHERE event_id = NEW.event_id AND effect_kind = NEW.effect_kind;
END;

CREATE TRIGGER routing_circuit_events_v3_advance_ingestion_sequence
AFTER INSERT ON routing_circuit_event_v3
WHEN NEW.ingestion_sequence IS NOT NULL
 AND NEW.ingestion_sequence >= (SELECT next_sequence FROM routing_v3_ingestion_sequence WHERE singleton_key = 1)
BEGIN
    UPDATE routing_v3_ingestion_sequence
    SET next_sequence = NEW.ingestion_sequence + 1
    WHERE singleton_key = 1;
END;

CREATE TABLE routing_circuit_generation_v3 (
    circuit_generation_id TEXT PRIMARY KEY CHECK (length(circuit_generation_id) BETWEEN 5 AND 192),
    scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 192),
    circuit_policy_revision INTEGER NOT NULL CHECK (circuit_policy_revision > 0),
    circuit_algorithm_version TEXT NOT NULL CHECK (length(circuit_algorithm_version) BETWEEN 1 AND 96),
    status TEXT NOT NULL CHECK (status IN ('building', 'ready', 'active', 'retired', 'failed')),
    input_circuit_event_watermark INTEGER CHECK (input_circuit_event_watermark IS NULL OR input_circuit_event_watermark >= 0),
    input_circuit_event_hash TEXT CHECK (input_circuit_event_hash IS NULL OR length(input_circuit_event_hash) = 64),
    output_content_hash TEXT CHECK (output_content_hash IS NULL OR length(output_content_hash) = 64),
    checkpoint_ref TEXT CHECK (checkpoint_ref IS NULL OR length(checkpoint_ref) BETWEEN 1 AND 192),
    processed_event_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_event_count >= 0),
    cursor_station_key_id TEXT,
    cursor_station_key_lifecycle_revision INTEGER,
    cursor_reducer_commit_sequence INTEGER,
    cursor_event_id TEXT,
    failure_code TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    ready_at_ms INTEGER CHECK (ready_at_ms IS NULL OR ready_at_ms >= created_at_ms),
    activated_at_ms INTEGER CHECK (activated_at_ms IS NULL OR activated_at_ms >= created_at_ms),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    CHECK (status IN ('building', 'failed') OR (
        input_circuit_event_watermark IS NOT NULL
        AND input_circuit_event_hash IS NOT NULL
        AND output_content_hash IS NOT NULL
        AND checkpoint_ref IS NOT NULL
    )),
    CHECK ((status = 'ready' AND ready_at_ms IS NOT NULL) OR status <> 'ready'),
    CHECK (status <> 'failed' OR failure_code IS NOT NULL)
);

CREATE TRIGGER routing_circuit_generation_v3_immutable_identity
BEFORE UPDATE OF circuit_generation_id, scope, circuit_policy_revision,
    circuit_algorithm_version, input_circuit_event_watermark,
    input_circuit_event_hash, output_content_hash
ON routing_circuit_generation_v3
WHEN OLD.circuit_generation_id IS NOT NEW.circuit_generation_id
 OR OLD.scope IS NOT NEW.scope
 OR OLD.circuit_policy_revision IS NOT NEW.circuit_policy_revision
 OR OLD.circuit_algorithm_version IS NOT NEW.circuit_algorithm_version
 OR OLD.input_circuit_event_watermark IS NOT NEW.input_circuit_event_watermark
 OR OLD.input_circuit_event_hash IS NOT NEW.input_circuit_event_hash
 OR OLD.output_content_hash IS NOT NEW.output_content_hash
BEGIN
    SELECT RAISE(ABORT, 'circuit generation identity is immutable');
END;

CREATE TABLE routing_circuit_generation_v3_checkpoint (
    circuit_generation_id TEXT PRIMARY KEY REFERENCES routing_circuit_generation_v3(circuit_generation_id) ON DELETE CASCADE,
    input_circuit_event_watermark INTEGER CHECK (input_circuit_event_watermark IS NULL OR input_circuit_event_watermark >= 0),
    cursor_station_key_id TEXT,
    cursor_station_key_lifecycle_revision INTEGER,
    cursor_reducer_commit_sequence INTEGER,
    cursor_event_id TEXT,
    processed_event_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_event_count >= 0),
    status TEXT NOT NULL CHECK (status IN ('building', 'ready', 'failed')),
    error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 96),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

-- Rebuild and rollback state is physically isolated from the mutable active
-- admission table above.  The coordinator changes only the runtime pointer;
-- it never overwrites another generation's reducer output.
CREATE TABLE routing_circuit_state_generation_v3 (
    circuit_generation_id TEXT NOT NULL
        REFERENCES routing_circuit_generation_v3(circuit_generation_id) ON DELETE CASCADE,
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    state TEXT NOT NULL CHECK (state IN ('closed', 'open', 'half_open')),
    state_revision INTEGER NOT NULL CHECK (state_revision > 0),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    reopen_level INTEGER NOT NULL DEFAULT 0 CHECK (reopen_level >= 0),
    opened_at_ms INTEGER CHECK (opened_at_ms IS NULL OR opened_at_ms >= 0),
    cooldown_until_ms INTEGER CHECK (cooldown_until_ms IS NULL OR cooldown_until_ms >= 0),
    recovery_successes INTEGER NOT NULL DEFAULT 0 CHECK (recovery_successes >= 0),
    monotonic_clock_watermark_ms INTEGER NOT NULL DEFAULT 0 CHECK (monotonic_clock_watermark_ms >= 0),
    reducer_commit_sequence INTEGER NOT NULL DEFAULT 0 CHECK (reducer_commit_sequence >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    PRIMARY KEY (
        circuit_generation_id, station_key_id,
        station_key_lifecycle_revision
    ),
    CHECK (
        (state = 'closed' AND opened_at_ms IS NULL AND cooldown_until_ms IS NULL)
        OR (state = 'open' AND opened_at_ms IS NOT NULL
            AND cooldown_until_ms IS NOT NULL
            AND cooldown_until_ms >= opened_at_ms)
        OR (state = 'half_open' AND opened_at_ms IS NOT NULL
            AND cooldown_until_ms IS NOT NULL)
    )
);

CREATE INDEX idx_routing_circuit_state_generation_v3_state
    ON routing_circuit_state_generation_v3(
        circuit_generation_id, state, station_key_id,
        station_key_lifecycle_revision
    );

CREATE TABLE routing_circuit_event_applied_generation_v3 (
    circuit_generation_id TEXT NOT NULL
        REFERENCES routing_circuit_generation_v3(circuit_generation_id) ON DELETE CASCADE,
    event_id TEXT NOT NULL CHECK (length(event_id) BETWEEN 1 AND 160),
    effect_kind TEXT NOT NULL CHECK (effect_kind IN ('observation', 'circuit', 'lease')),
    station_key_id TEXT NOT NULL CHECK (length(station_key_id) BETWEEN 1 AND 160),
    station_key_lifecycle_revision INTEGER NOT NULL CHECK (station_key_lifecycle_revision > 0),
    reducer_commit_sequence INTEGER NOT NULL CHECK (reducer_commit_sequence > 0),
    ingestion_sequence INTEGER NOT NULL CHECK (ingestion_sequence > 0),
    applied_at_ms INTEGER NOT NULL CHECK (applied_at_ms >= 0),
    PRIMARY KEY (circuit_generation_id, event_id, effect_kind),
    UNIQUE (
        circuit_generation_id, station_key_id,
        station_key_lifecycle_revision, reducer_commit_sequence
    )
);

UPDATE persistence_schema_compatibility
SET schema_version = 62,
    updated_by_migration = 62,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 62;

CREATE TEMP TABLE persistence_v62_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 62)
);
INSERT INTO persistence_v62_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v62_schema_guard;
