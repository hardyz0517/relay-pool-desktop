ALTER TABLE station_keys ADD COLUMN routing_order INTEGER;

ALTER TABLE station_key_health ADD COLUMN last_success_at TEXT;
ALTER TABLE station_key_health ADD COLUMN last_failure_at TEXT;
ALTER TABLE station_key_health ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE station_key_health ADD COLUMN success_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE station_key_health ADD COLUMN failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE station_key_health ADD COLUMN total_duration_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE station_key_health ADD COLUMN avg_latency_ms INTEGER;
ALTER TABLE station_key_health ADD COLUMN last_error_summary TEXT;
ALTER TABLE station_key_health ADD COLUMN cooldown_until TEXT;
ALTER TABLE station_key_health ADD COLUMN updated_at TEXT NOT NULL DEFAULT '0';

CREATE INDEX idx_station_keys_routing_order
    ON station_keys(routing_order ASC, priority ASC, created_at ASC, id ASC);

CREATE TABLE station_key_capabilities (
    station_key_id TEXT PRIMARY KEY REFERENCES station_keys(id) ON DELETE CASCADE,
    supports_chat_completions INTEGER NOT NULL DEFAULT 1,
    supports_responses INTEGER NOT NULL DEFAULT 1,
    supports_embeddings INTEGER NOT NULL DEFAULT 0,
    supports_stream INTEGER NOT NULL DEFAULT 1,
    supports_tools INTEGER NOT NULL DEFAULT 0,
    supports_vision INTEGER NOT NULL DEFAULT 0,
    supports_reasoning INTEGER NOT NULL DEFAULT 0,
    model_allowlist_json TEXT NOT NULL DEFAULT '[]',
    model_blocklist_json TEXT NOT NULL DEFAULT '[]',
    preferred_models_json TEXT NOT NULL DEFAULT '[]',
    only_use_as_backup INTEGER NOT NULL DEFAULT 0,
    routing_tags_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL
);

CREATE TABLE model_aliases (
    id TEXT PRIMARY KEY,
    client_model TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_model_aliases_client_upstream
    ON model_aliases(client_model, upstream_model);

CREATE TABLE pricing_rules (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE SET NULL,
    group_binding_id TEXT,
    group_name TEXT,
    tier_label TEXT,
    model TEXT NOT NULL,
    input_price REAL,
    output_price REAL,
    fixed_price REAL,
    rate_multiplier REAL,
    currency TEXT NOT NULL,
    unit TEXT NOT NULL,
    price_type TEXT NOT NULL,
    base_price_source TEXT,
    normalization_status TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    enabled INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    collected_at TEXT,
    valid_from TEXT,
    valid_until TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_pricing_rules_station_model
    ON pricing_rules(station_id, model, enabled, updated_at DESC);

CREATE TABLE balance_snapshots (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    station_key_id TEXT REFERENCES station_keys(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    value REAL,
    currency TEXT NOT NULL,
    credit_unit TEXT,
    used_value REAL,
    total_value REAL,
    today_request_count INTEGER,
    total_request_count INTEGER,
    today_consumption REAL,
    total_consumption REAL,
    today_base_consumption REAL,
    total_base_consumption REAL,
    today_token_count INTEGER,
    total_token_count INTEGER,
    today_input_token_count INTEGER,
    today_output_token_count INTEGER,
    total_input_token_count INTEGER,
    total_output_token_count INTEGER,
    account_concurrency_limit INTEGER,
    low_balance_threshold REAL,
    status TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    collected_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_balance_snapshots_station_scope_updated
    ON balance_snapshots(station_id, scope, updated_at DESC);

UPDATE persistence_schema_compatibility
SET schema_version = 4,
    updated_by_migration = 4,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
