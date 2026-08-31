-- Routing policy v3 staging schema.
--
-- This migration is structure-only. Policy conversion, canonical JSON hashing
-- and deterministic generation IDs belong to the Rust staging owner; SQLite
-- must never invent an identity from a mutable revision or wall-clock value.

CREATE TABLE routing_policy_v3_staged (
    staged_id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope TEXT NOT NULL CHECK (scope IN ('active', 'history')),
    source_config_revision INTEGER NOT NULL CHECK (source_config_revision > 0),
    target_policy_revision INTEGER NOT NULL CHECK (target_policy_revision > 0),
    -- Compatibility aliases for generation readers. Both are immutable and
    -- constrained to the canonical target fields above.
    config_revision INTEGER NOT NULL CHECK (config_revision = target_policy_revision),
    policy_generation_id TEXT NOT NULL
        CHECK (length(policy_generation_id) = 68 AND policy_generation_id GLOB 'pg1_[0-9a-f]*'),
    canonical_policy_hash TEXT NOT NULL
        CHECK (length(canonical_policy_hash) = 64 AND canonical_policy_hash NOT GLOB '*[^0-9a-f]*'),
    policy_algorithm_version TEXT NOT NULL
        CHECK (length(policy_algorithm_version) BETWEEN 1 AND 96),
    source_policy_version TEXT NOT NULL
        CHECK (length(source_policy_version) BETWEEN 1 AND 96),
    system_version TEXT NOT NULL CHECK (length(system_version) BETWEEN 1 AND 96),
    target_policy_version TEXT NOT NULL CHECK (target_policy_version = 'routing-policy-v3'),
    staged_policy_version TEXT NOT NULL
        CHECK (staged_policy_version = target_policy_version),
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    status TEXT NOT NULL CHECK (status IN ('staged', 'ready', 'active', 'retired', 'failed')),
    failure_code TEXT CHECK (
        failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 96
    ),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    UNIQUE (policy_generation_id),
    UNIQUE (scope, source_config_revision, target_policy_revision, target_policy_version),
    UNIQUE (scope, target_policy_revision),
    CHECK ((status = 'failed') = (failure_code IS NOT NULL))
);

CREATE INDEX idx_routing_policy_v3_staged_status
    ON routing_policy_v3_staged(scope, status, target_policy_revision DESC);

CREATE TRIGGER routing_policy_v3_staged_immutable_payload
BEFORE UPDATE OF scope, source_config_revision, target_policy_revision,
    config_revision, policy_generation_id, canonical_policy_hash,
    policy_algorithm_version, source_policy_version, target_policy_version,
    system_version, staged_policy_version, config_json, created_at_ms
ON routing_policy_v3_staged
BEGIN
    SELECT RAISE(ABORT, 'routing policy staged payload is immutable');
END;

CREATE TRIGGER routing_policy_v3_staged_legal_status_transition
BEFORE UPDATE OF status ON routing_policy_v3_staged
WHEN OLD.status <> NEW.status
 AND NOT (
    (OLD.status = 'staged' AND NEW.status IN ('ready', 'failed'))
    OR (OLD.status = 'ready' AND NEW.status IN ('active', 'failed'))
    OR (OLD.status = 'active' AND NEW.status = 'retired')
    OR (OLD.status = 'retired' AND NEW.status = 'active')
 )
BEGIN
    SELECT RAISE(ABORT, 'illegal routing policy staged status transition');
END;

CREATE TABLE routing_policy_v3_migration_audit (
    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope TEXT NOT NULL CHECK (scope IN ('active', 'history')),
    source_config_revision INTEGER NOT NULL CHECK (source_config_revision > 0),
    target_policy_revision INTEGER NOT NULL CHECK (target_policy_revision > 0),
    target_policy_version TEXT NOT NULL CHECK (target_policy_version = 'routing-policy-v3'),
    policy_generation_id TEXT NOT NULL
        REFERENCES routing_policy_v3_staged(policy_generation_id) ON DELETE RESTRICT,
    migration_status TEXT NOT NULL
        CHECK (migration_status IN ('staged', 'ready', 'active', 'failed')),
    source_fields_json TEXT NOT NULL CHECK (json_valid(source_fields_json)),
    defaulted_fields_json TEXT NOT NULL CHECK (json_valid(defaulted_fields_json)),
    discarded_fields_json TEXT NOT NULL CHECK (json_valid(discarded_fields_json)),
    semantic_changes_json TEXT NOT NULL CHECK (json_valid(semantic_changes_json)),
    quality_rebuild_required INTEGER NOT NULL
        CHECK (quality_rebuild_required IN (0, 1)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (scope, source_config_revision, target_policy_revision, target_policy_version),
    UNIQUE (policy_generation_id)
);

CREATE TRIGGER routing_policy_v3_migration_audit_no_update
BEFORE UPDATE ON routing_policy_v3_migration_audit
BEGIN
    SELECT RAISE(ABORT, 'routing policy migration audit is append-only');
END;

CREATE TRIGGER routing_policy_v3_migration_audit_no_delete
BEFORE DELETE ON routing_policy_v3_migration_audit
BEGIN
    SELECT RAISE(ABORT, 'routing policy migration audit is append-only');
END;

UPDATE persistence_schema_compatibility
SET schema_version = 60,
    updated_by_migration = 60,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 60;

CREATE TEMP TABLE persistence_v60_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 60)
);
INSERT INTO persistence_v60_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v60_schema_guard;
