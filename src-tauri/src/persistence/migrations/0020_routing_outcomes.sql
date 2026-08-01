CREATE TABLE IF NOT EXISTS routing_attempt_costs (
    request_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    pricing_context_id TEXT NOT NULL CHECK (trim(pricing_context_id) <> ''),
    pricing_basis TEXT NOT NULL CHECK (pricing_basis IN ('exact_price', 'multiplier_proxy', 'unpriced', 'not_applicable')),
    pricing_status_label TEXT NOT NULL CHECK (trim(pricing_status_label) <> ''),
    usage_status TEXT NOT NULL CHECK (usage_status IN ('complete', 'missing_usage', 'stream_usage_missing', 'not_applicable')),
    input_tokens INTEGER,
    output_tokens INTEGER,
    total_tokens INTEGER,
    cache_creation_tokens INTEGER,
    cache_read_tokens INTEGER,
    cost_status TEXT NOT NULL CHECK (cost_status IN ('priced', 'missing_usage', 'stream_usage_missing', 'unpriced', 'pricing_incomplete', 'not_applicable')),
    currency TEXT,
    total_cost_micro INTEGER,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (request_id, ordinal),
    FOREIGN KEY (request_id, ordinal) REFERENCES request_attempts(request_id, ordinal) ON DELETE CASCADE,
    CHECK (
        (cost_status = 'priced' AND currency IS NOT NULL AND total_cost_micro IS NOT NULL AND usage_status = 'complete')
        OR (cost_status <> 'priced' AND currency IS NULL AND total_cost_micro IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_routing_attempt_costs_request
    ON routing_attempt_costs(request_id, ordinal ASC);

CREATE TABLE IF NOT EXISTS routing_request_cost_aggregates (
    request_id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('no_attempts', 'complete_single_currency', 'complete_mixed_currency', 'incomplete', 'not_applicable')),
    totals_by_currency_json TEXT NOT NULL CHECK (json_valid(totals_by_currency_json)),
    compatibility_currency TEXT,
    compatibility_total_cost_micro INTEGER,
    incomplete_attempts_json TEXT NOT NULL CHECK (json_valid(incomplete_attempts_json)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK (
        (status = 'complete_single_currency' AND compatibility_currency IS NOT NULL AND compatibility_total_cost_micro IS NOT NULL)
        OR (status <> 'complete_single_currency' AND compatibility_currency IS NULL AND compatibility_total_cost_micro IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_routing_request_cost_aggregates_updated
    ON routing_request_cost_aggregates(updated_at_ms DESC, request_id ASC);

CREATE TABLE IF NOT EXISTS routing_lifecycle_reconciliation_progress (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    last_request_id TEXT,
    last_run_at_ms INTEGER NOT NULL,
    batches_completed INTEGER NOT NULL DEFAULT 0 CHECK (batches_completed >= 0),
    requests_interrupted INTEGER NOT NULL DEFAULT 0 CHECK (requests_interrupted >= 0),
    attempt_cost_gaps_inserted INTEGER NOT NULL DEFAULT 0 CHECK (attempt_cost_gaps_inserted >= 0),
    decisions_marked_trace_incomplete INTEGER NOT NULL DEFAULT 0 CHECK (decisions_marked_trace_incomplete >= 0),
    completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1))
);

UPDATE persistence_schema_compatibility
SET schema_version = 20,
    updated_by_migration = 20,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 20;
