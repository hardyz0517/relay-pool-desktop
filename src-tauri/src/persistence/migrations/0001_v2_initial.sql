CREATE TABLE persistence_schema_compatibility (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    database_generation INTEGER NOT NULL CHECK (database_generation = 2),
    schema_version INTEGER NOT NULL CHECK (schema_version >= 1),
    min_reader_app_version TEXT NOT NULL,
    min_writer_app_version TEXT NOT NULL,
    updated_by_migration INTEGER NOT NULL CHECK (updated_by_migration >= 1),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO persistence_schema_compatibility (
    singleton_key,
    database_generation,
    schema_version,
    min_reader_app_version,
    min_writer_app_version,
    updated_by_migration
) VALUES (
    1,
    2,
    1,
    '0.3.1',
    '0.3.1',
    1
);

CREATE TABLE persistence_runtime_health (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    write_probe_count INTEGER NOT NULL DEFAULT 0 CHECK (write_probe_count >= 0),
    last_open_mode TEXT NOT NULL DEFAULT 'never' CHECK (last_open_mode IN ('never', 'writable')),
    last_checked_at TEXT
);

INSERT INTO persistence_runtime_health (singleton_key) VALUES (1);
