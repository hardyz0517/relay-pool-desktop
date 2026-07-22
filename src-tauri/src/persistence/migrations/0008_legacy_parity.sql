ALTER TABLE station_credentials ADD COLUMN login_username TEXT;
ALTER TABLE station_credentials ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

ALTER TABLE station_endpoint_health ADD COLUMN status TEXT NOT NULL DEFAULT 'unchecked';
ALTER TABLE station_endpoint_health ADD COLUMN latency_ms INTEGER;
ALTER TABLE station_endpoint_health ADD COLUMN checked_at TEXT;
ALTER TABLE station_endpoint_health ADD COLUMN error_summary TEXT;
ALTER TABLE station_endpoint_health ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

ALTER TABLE request_logs ADD COLUMN billing_mode TEXT;
ALTER TABLE request_logs ADD COLUMN estimated_input_cost REAL;
ALTER TABLE request_logs ADD COLUMN estimated_output_cost REAL;
ALTER TABLE request_logs ADD COLUMN estimated_total_cost REAL;
ALTER TABLE request_logs ADD COLUMN base_input_cost REAL;
ALTER TABLE request_logs ADD COLUMN base_output_cost REAL;
ALTER TABLE request_logs ADD COLUMN base_fixed_cost REAL;
ALTER TABLE request_logs ADD COLUMN base_total_cost REAL;
ALTER TABLE request_logs ADD COLUMN cost_currency TEXT;
ALTER TABLE request_logs ADD COLUMN pricing_rule_id TEXT;
ALTER TABLE request_logs ADD COLUMN pricing_source TEXT;
ALTER TABLE request_logs ADD COLUMN cost_status TEXT;
ALTER TABLE request_logs ADD COLUMN group_binding_id TEXT;
ALTER TABLE request_logs ADD COLUMN normalization_status TEXT;
ALTER TABLE request_logs ADD COLUMN balance_scope TEXT;
ALTER TABLE request_logs ADD COLUMN economic_context_json TEXT;

UPDATE persistence_schema_compatibility
SET schema_version = 8,
    updated_by_migration = 8,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 7;
