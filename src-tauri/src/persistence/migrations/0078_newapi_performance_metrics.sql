-- Persist provider-reported performance metrics when a source exposes them.
-- Sub2API leaves these nullable; NewAPI fills them from /api/perf-metrics.
ALTER TABLE station_published_monitors ADD COLUMN current_ttft_ms INTEGER
    CHECK (current_ttft_ms IS NULL OR current_ttft_ms >= 0);
ALTER TABLE station_published_monitors ADD COLUMN current_tps REAL
    CHECK (current_tps IS NULL OR (current_tps >= 0 AND current_tps <= 1000000));

ALTER TABLE station_published_monitor_samples ADD COLUMN ttft_ms INTEGER
    CHECK (ttft_ms IS NULL OR ttft_ms >= 0);
ALTER TABLE station_published_monitor_samples ADD COLUMN tps REAL
    CHECK (tps IS NULL OR (tps >= 0 AND tps <= 1000000));

UPDATE persistence_schema_compatibility
SET schema_version = 78,
    updated_by_migration = 78,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version < 78;

CREATE TEMP TABLE persistence_v78_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 78)
);

INSERT INTO persistence_v78_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v78_schema_guard;
