CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO settings (key, value, updated_at) VALUES
    ('local_proxy_port', '8787', strftime('%s', 'now')),
    ('local_key', 'sk-local-pool-change-me', strftime('%s', 'now')),
    ('default_routing_strategy', 'cost_stable_first', strftime('%s', 'now')),
    ('collector_proxy_mode', 'direct', strftime('%s', 'now')),
    ('collector_proxy_url', '', strftime('%s', 'now')),
    ('max_rate_multiplier', '', strftime('%s', 'now')),
    ('default_routing_group_filter', 'all_groups', strftime('%s', 'now')),
    ('scheduler_advanced_settings_json', '', strftime('%s', 'now')),
    ('low_balance_threshold_cny', '15', strftime('%s', 'now')),
    ('collector_interval_minutes', '30', strftime('%s', 'now')),
    ('balance_interval_minutes', '5', strftime('%s', 'now')),
    ('group_rate_interval_minutes', '20', strftime('%s', 'now')),
    ('model_list_interval_minutes', '60', strftime('%s', 'now')),
    ('pricing_refresh_interval_minutes', '60', strftime('%s', 'now')),
    ('collector_timeout_seconds', '15', strftime('%s', 'now')),
    ('collector_max_concurrency', '3', strftime('%s', 'now')),
    ('allow_depleted_fallback', 'false', strftime('%s', 'now')),
    ('developer_mode_enabled', 'false', strftime('%s', 'now')),
    ('tray_behavior', 'close_to_tray', strftime('%s', 'now'));

CREATE TABLE secrets (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    masked_value TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(scope, owner_id, kind)
);

CREATE TABLE stations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    station_type TEXT NOT NULL,
    website_url TEXT NOT NULL,
    api_base_url TEXT NOT NULL,
    endpoint_revision INTEGER NOT NULL DEFAULT 1 CHECK (endpoint_revision >= 1),
    api_key TEXT NOT NULL DEFAULT '',
    api_key_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL,
    upstream_api_format TEXT NOT NULL DEFAULT 'auto',
    collector_proxy_mode TEXT NOT NULL DEFAULT 'inherit',
    collector_proxy_url TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    priority INTEGER NOT NULL DEFAULT 0,
    credit_per_cny REAL NOT NULL DEFAULT 1.0 CHECK (credit_per_cny > 0),
    balance_raw REAL,
    balance_cny REAL,
    low_balance_threshold_cny REAL,
    collection_interval_minutes INTEGER NOT NULL DEFAULT 30 CHECK (collection_interval_minutes > 0),
    status TEXT NOT NULL DEFAULT 'unchecked',
    latency_ms INTEGER,
    last_checked_at TEXT,
    last_pricing_fetched_at TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_stations_order ON stations(priority, created_at);

CREATE TABLE station_keys (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL REFERENCES stations(id) ON DELETE CASCADE
);

CREATE INDEX idx_station_keys_station_id ON station_keys(station_id);

CREATE TABLE station_endpoint_health (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE station_key_health (
    station_key_id TEXT PRIMARY KEY REFERENCES station_keys(id) ON DELETE CASCADE,
    endpoint_revision INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE station_credentials (
    station_id TEXT PRIMARY KEY REFERENCES stations(id) ON DELETE CASCADE,
    login_password TEXT,
    login_password_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL,
    remember_password INTEGER NOT NULL DEFAULT 0 CHECK (remember_password IN (0, 1)),
    login_status TEXT NOT NULL DEFAULT 'unknown',
    login_error TEXT,
    last_login_at TEXT,
    session_status TEXT NOT NULL DEFAULT 'none',
    session_expires_at TEXT,
    access_token_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL,
    refresh_token_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL,
    cookie_secret_id TEXT REFERENCES secrets(id) ON DELETE SET NULL,
    newapi_user_id TEXT,
    token_expires_at TEXT,
    token_refreshed_at TEXT,
    session_source TEXT NOT NULL DEFAULT 'none',
    updated_at TEXT NOT NULL
);

UPDATE persistence_schema_compatibility
SET schema_version = 2,
    updated_by_migration = 2,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1;
