-- Versioned, redacted terminal routing facts. This table deliberately has no
-- request body, upstream URL, provider identity, message, or credential data.
CREATE TABLE IF NOT EXISTS request_routing_outcome_summaries (
    request_id TEXT PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
    profile_version TEXT NOT NULL CHECK (profile_version = 'routing_outcome_v1'),
    terminal_kind TEXT NOT NULL CHECK (terminal_kind IN ('completed', 'partial_success', 'failed', 'interrupted')),
    terminal_code TEXT NOT NULL CHECK (length(terminal_code) BETWEEN 1 AND 96 AND terminal_code GLOB '[a-z0-9_]*'),
    classification TEXT NOT NULL CHECK (classification IN ('success', 'generic', 'authentication', 'balance', 'rate_limit', 'capacity', 'model_not_found', 'server_error', 'transport', 'timeout', 'protocol', 'downstream', 'local')),
    confidence TEXT NOT NULL CHECK (confidence IN ('confirmed', 'probable', 'unknown', 'conflicting', 'not_applicable')),
    evidence_source TEXT NOT NULL CHECK (evidence_source IN ('none', 'http_status', 'error_envelope', 'sse_event', 'transport', 'timeout', 'local', 'downstream')),
    request_accepted TEXT NOT NULL CHECK (request_accepted IN ('accepted', 'not_accepted', 'unknown')),
    send_phase TEXT NOT NULL CHECK (send_phase IN ('not_connected', 'unknown', 'response_started')),
    replay_disposition TEXT NOT NULL CHECK (replay_disposition IN ('not_applicable', 'stopped_after_commit', 'stopped_uncertain', 'completed')),
    billing_state TEXT NOT NULL CHECK (billing_state IN ('not_applicable', 'not_billed', 'possibly_billed', 'completed')),
    retry_disposition TEXT NOT NULL CHECK (retry_disposition IN ('none', 'same_target_exhausted', 'fail_closed')),
    effect_summary TEXT NOT NULL CHECK (effect_summary IN ('none', 'health_or_capability_applied', 'neutral')),
    failure_domain_commitment_version INTEGER CHECK (failure_domain_commitment_version IS NULL OR failure_domain_commitment_version = 1),
    failure_domain_commitment_digest TEXT CHECK (failure_domain_commitment_digest IS NULL OR (length(failure_domain_commitment_digest) = 64 AND failure_domain_commitment_digest GLOB '[0-9a-f]*')),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    fallback_count INTEGER NOT NULL CHECK (fallback_count >= 0),
    terminal_at_ms INTEGER NOT NULL
    ,CHECK ((failure_domain_commitment_version IS NULL) = (failure_domain_commitment_digest IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_request_routing_outcome_summaries_terminal
    ON request_routing_outcome_summaries(terminal_at_ms DESC, request_id ASC);

UPDATE persistence_schema_compatibility
SET schema_version = 37,
    updated_by_migration = 37,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 37;
