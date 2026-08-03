CREATE TABLE IF NOT EXISTS dashboard_request_metric_rollups (
    bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
    bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
    request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    terminal_count INTEGER NOT NULL DEFAULT 0 CHECK (terminal_count >= 0),
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    interrupted_count INTEGER NOT NULL DEFAULT 0 CHECK (interrupted_count >= 0),
    in_progress_count INTEGER NOT NULL DEFAULT 0 CHECK (in_progress_count >= 0),
    prompt_tokens INTEGER NOT NULL DEFAULT 0 CHECK (prompt_tokens >= 0),
    completion_tokens INTEGER NOT NULL DEFAULT 0 CHECK (completion_tokens >= 0),
    total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    known_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (known_usage_request_count >= 0),
    missing_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (missing_usage_request_count >= 0),
    stream_usage_missing_request_count INTEGER NOT NULL DEFAULT 0 CHECK (stream_usage_missing_request_count >= 0),
    not_applicable_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (not_applicable_usage_request_count >= 0),
    unknown_usage_request_count INTEGER NOT NULL DEFAULT 0 CHECK (unknown_usage_request_count >= 0),
    total_duration_ms INTEGER NOT NULL DEFAULT 0 CHECK (total_duration_ms >= 0),
    invalid_duration_count INTEGER NOT NULL DEFAULT 0 CHECK (invalid_duration_count >= 0),
    duration_sample_count INTEGER NOT NULL DEFAULT 0 CHECK (duration_sample_count >= 0),
    first_token_total_ms INTEGER NOT NULL DEFAULT 0 CHECK (first_token_total_ms >= 0),
    first_token_sample_count INTEGER NOT NULL DEFAULT 0 CHECK (first_token_sample_count >= 0),
    unknown_lifecycle_count INTEGER NOT NULL DEFAULT 0 CHECK (unknown_lifecycle_count >= 0),
    PRIMARY KEY (bucket_kind, bucket_start_ms)
);

CREATE INDEX IF NOT EXISTS idx_dashboard_request_metric_rollups_range
    ON dashboard_request_metric_rollups(bucket_kind, bucket_start_ms);

CREATE TABLE IF NOT EXISTS dashboard_request_cost_rollups (
    bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
    bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
    legacy_or_missing_aggregate_count INTEGER NOT NULL DEFAULT 0 CHECK (legacy_or_missing_aggregate_count >= 0),
    complete_single_currency_count INTEGER NOT NULL DEFAULT 0 CHECK (complete_single_currency_count >= 0),
    complete_mixed_currency_count INTEGER NOT NULL DEFAULT 0 CHECK (complete_mixed_currency_count >= 0),
    incomplete_count INTEGER NOT NULL DEFAULT 0 CHECK (incomplete_count >= 0),
    not_applicable_count INTEGER NOT NULL DEFAULT 0 CHECK (not_applicable_count >= 0),
    no_attempts_count INTEGER NOT NULL DEFAULT 0 CHECK (no_attempts_count >= 0),
    corrupt_cost_aggregate_count INTEGER NOT NULL DEFAULT 0 CHECK (corrupt_cost_aggregate_count >= 0),
    PRIMARY KEY (bucket_kind, bucket_start_ms)
);

CREATE INDEX IF NOT EXISTS idx_dashboard_request_cost_rollups_range
    ON dashboard_request_cost_rollups(bucket_kind, bucket_start_ms);

CREATE TABLE IF NOT EXISTS dashboard_request_cost_totals_rollups (
    bucket_kind TEXT NOT NULL CHECK (bucket_kind IN ('second', 'lifetime')),
    bucket_start_ms INTEGER NOT NULL CHECK (bucket_start_ms >= 0),
    currency TEXT NOT NULL CHECK (
        length(currency) BETWEEN 3 AND 16
        AND currency = upper(currency)
        AND currency NOT GLOB '*[^A-Z]*'
    ),
    amount_micro INTEGER NOT NULL DEFAULT 0 CHECK (amount_micro >= 0),
    request_count INTEGER NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    PRIMARY KEY (bucket_kind, bucket_start_ms, currency)
);

CREATE INDEX IF NOT EXISTS idx_dashboard_request_cost_totals_rollups_range
    ON dashboard_request_cost_totals_rollups(bucket_kind, bucket_start_ms, currency);

UPDATE persistence_schema_compatibility
SET schema_version = 23,
    updated_by_migration = 23,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 23;
