ALTER TABLE model_base_prices ADD COLUMN input_price_priority REAL
CHECK (input_price_priority IS NULL OR input_price_priority >= 0);

ALTER TABLE model_base_prices ADD COLUMN output_price_priority REAL
CHECK (output_price_priority IS NULL OR output_price_priority >= 0);

ALTER TABLE model_base_prices ADD COLUMN cache_creation_price REAL
CHECK (cache_creation_price IS NULL OR cache_creation_price >= 0);

ALTER TABLE model_base_prices ADD COLUMN cache_creation_price_priority REAL
CHECK (cache_creation_price_priority IS NULL OR cache_creation_price_priority >= 0);

ALTER TABLE model_base_prices ADD COLUMN cache_creation_price_above_1hr REAL
CHECK (cache_creation_price_above_1hr IS NULL OR cache_creation_price_above_1hr >= 0);

ALTER TABLE model_base_prices ADD COLUMN cache_read_price REAL
CHECK (cache_read_price IS NULL OR cache_read_price >= 0);

ALTER TABLE model_base_prices ADD COLUMN cache_read_price_priority REAL
CHECK (cache_read_price_priority IS NULL OR cache_read_price_priority >= 0);

ALTER TABLE model_base_prices ADD COLUMN long_context_input_token_threshold INTEGER
CHECK (long_context_input_token_threshold IS NULL OR long_context_input_token_threshold > 0);

ALTER TABLE model_base_prices ADD COLUMN long_context_input_cost_multiplier REAL
CHECK (long_context_input_cost_multiplier IS NULL OR long_context_input_cost_multiplier > 0);

ALTER TABLE model_base_prices ADD COLUMN long_context_output_cost_multiplier REAL
CHECK (long_context_output_cost_multiplier IS NULL OR long_context_output_cost_multiplier > 0);

ALTER TABLE model_base_prices ADD COLUMN supports_service_tier INTEGER NOT NULL DEFAULT 0
CHECK (supports_service_tier IN (0, 1));

ALTER TABLE model_base_prices ADD COLUMN supports_prompt_caching INTEGER NOT NULL DEFAULT 0
CHECK (supports_prompt_caching IN (0, 1));

UPDATE persistence_schema_compatibility
SET schema_version = 33,
    updated_by_migration = 33,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 32;

CREATE TEMP TABLE persistence_v33_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 33)
);

INSERT INTO persistence_v33_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v33_schema_guard;
