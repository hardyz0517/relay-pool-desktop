CREATE TABLE IF NOT EXISTS route_decisions (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    decided_at_ms INTEGER NOT NULL,
    ordering_profile TEXT NOT NULL,
    selected_station_key_id TEXT,
    selected_station_id TEXT,
    selected_endpoint_revision INTEGER,
    candidate_count INTEGER NOT NULL,
    candidate_detail_count INTEGER NOT NULL,
    candidate_detail_truncated INTEGER NOT NULL DEFAULT 0 CHECK (candidate_detail_truncated IN (0, 1)),
    rejection_counts_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    fact_version_vector TEXT NOT NULL,
    planner_version TEXT NOT NULL,
    projector_version TEXT NOT NULL,
    runtime_overlay_revision INTEGER NOT NULL,
    trace_status TEXT NOT NULL CHECK (trace_status IN ('complete', 'trace_incomplete')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_route_decisions_cursor
    ON route_decisions(decided_at_ms DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_route_decisions_created_at
    ON route_decisions(created_at_ms DESC, id DESC);

CREATE TABLE IF NOT EXISTS route_candidate_decisions (
    id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL REFERENCES route_decisions(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    station_key_id TEXT NOT NULL,
    station_id TEXT NOT NULL,
    endpoint_revision INTEGER NOT NULL,
    selected INTEGER NOT NULL DEFAULT 0 CHECK (selected IN (0, 1)),
    attempted INTEGER NOT NULL DEFAULT 0 CHECK (attempted IN (0, 1)),
    retained_reason TEXT NOT NULL,
    availability_tier TEXT NOT NULL,
    hard_rejection_code TEXT,
    hard_rejection_gate TEXT,
    priority INTEGER NOT NULL,
    cost_basis TEXT NOT NULL,
    cost_currency TEXT,
    cost_unit TEXT,
    cost_comparison_value REAL,
    snapshot_id TEXT NOT NULL,
    fact_version_vector TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_route_candidate_decisions_decision
    ON route_candidate_decisions(decision_id, selected DESC, attempted DESC, station_key_id ASC);

CREATE INDEX IF NOT EXISTS idx_route_candidate_decisions_request
    ON route_candidate_decisions(request_id, station_key_id ASC);

UPDATE persistence_schema_compatibility
SET schema_version = 19,
    updated_by_migration = 19,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 19;
