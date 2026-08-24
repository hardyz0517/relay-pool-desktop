-- Model mapping foundation.  This migration is intentionally additive: the
-- legacy model_aliases table remains available for the bounded audit window,
-- but no new runtime path is added here.

PRAGMA foreign_keys = ON;

CREATE TABLE model_mapping_policies (
    singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
    revision INTEGER NOT NULL CHECK (revision > 0),
    unmatched_model_behavior TEXT NOT NULL COLLATE BINARY
        CHECK (unmatched_model_behavior IN ('preserve', 'reject')),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

INSERT INTO model_mapping_policies (
    singleton_key, revision, unmatched_model_behavior, updated_at_ms
) VALUES (1, 1, 'preserve', 0);

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
VALUES ('model_mapping', 1, 0, 'baseline_snapshot')
ON CONFLICT(scope) DO NOTHING;

CREATE TABLE model_mapping_rules (
    id TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 192),
    priority INTEGER NOT NULL CHECK (priority BETWEEN -1000000 AND 1000000),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    matcher_kind TEXT NOT NULL COLLATE BINARY
        CHECK (matcher_kind IN ('exact', 'default', 'glob')),
    matcher_value TEXT COLLATE BINARY,
    endpoint_conditions_json TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(endpoint_conditions_json)
            AND json_type(endpoint_conditions_json) = 'array'
            AND length(CAST(endpoint_conditions_json AS BLOB)) <= 4096),
    stream_condition TEXT NOT NULL DEFAULT 'any' COLLATE BINARY
        CHECK (stream_condition IN ('any', 'required', 'forbidden')),
    tools_condition TEXT NOT NULL DEFAULT 'any' COLLATE BINARY
        CHECK (tools_condition IN ('any', 'required', 'forbidden')),
    vision_condition TEXT NOT NULL DEFAULT 'any' COLLATE BINARY
        CHECK (vision_condition IN ('any', 'required', 'forbidden')),
    reasoning_condition TEXT NOT NULL DEFAULT 'any' COLLATE BINARY
        CHECK (reasoning_condition IN ('any', 'required', 'forbidden')),
    action_kind TEXT NOT NULL COLLATE BINARY
        CHECK (action_kind IN ('map_fixed', 'map_fallback_chain', 'preserve', 'reject')),
    fallback_trigger TEXT COLLATE BINARY
        CHECK (fallback_trigger IS NULL
            OR fallback_trigger IN ('no_eligible_target', 'retry_exhausted_before_output')),
    note TEXT CHECK (note IS NULL OR
        (length(CAST(note AS BLOB)) <= 1024 AND instr(note, char(0)) = 0)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    revision INTEGER NOT NULL CHECK (revision > 0),
    CHECK (
        (matcher_kind = 'default' AND matcher_value IS NULL)
        OR (matcher_kind <> 'default' AND matcher_value IS NOT NULL
            AND length(CAST(matcher_value AS BLOB)) BETWEEN 1 AND 512
            AND instr(matcher_value, char(0)) = 0
            AND instr(matcher_value, char(10)) = 0
            AND instr(matcher_value, char(13)) = 0)
    ),
    CHECK (
        (action_kind IN ('map_fixed', 'preserve', 'reject') AND fallback_trigger IS NULL)
        OR (action_kind = 'map_fallback_chain' AND fallback_trigger IS NOT NULL)
    )
);

CREATE INDEX idx_model_mapping_rules_runtime
    ON model_mapping_rules(enabled, priority DESC, id COLLATE BINARY);

CREATE UNIQUE INDEX idx_model_mapping_rules_one_default
    ON model_mapping_rules(matcher_kind)
    WHERE matcher_kind = 'default' AND enabled = 1;

CREATE TABLE model_mapping_rule_targets (
    id TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 192),
    rule_id TEXT NOT NULL COLLATE BINARY
        REFERENCES model_mapping_rules(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position BETWEEN 0 AND 63),
    target_kind TEXT NOT NULL COLLATE BINARY
        CHECK (target_kind IN ('literal', 'model_profile')),
    literal_upstream_model TEXT COLLATE BINARY,
    model_profile_id TEXT COLLATE BINARY,
    UNIQUE (rule_id, position),
    FOREIGN KEY (model_profile_id) REFERENCES model_profiles(id) ON DELETE RESTRICT,
    CHECK (
        (target_kind = 'literal'
            AND literal_upstream_model IS NOT NULL
            AND length(CAST(literal_upstream_model AS BLOB)) BETWEEN 1 AND 512
            AND instr(literal_upstream_model, char(0)) = 0
            AND model_profile_id IS NULL)
        OR (target_kind = 'model_profile'
            AND model_profile_id IS NOT NULL
            AND literal_upstream_model IS NULL)
    )
);

CREATE INDEX idx_model_mapping_rule_targets_order
    ON model_mapping_rule_targets(rule_id, position ASC);

CREATE TABLE model_profiles (
    id TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 192),
    canonical_model TEXT NOT NULL COLLATE BINARY
        CHECK (length(CAST(canonical_model AS BLOB)) BETWEEN 1 AND 512
            AND instr(canonical_model, char(0)) = 0),
    display_name TEXT NOT NULL
        CHECK (length(CAST(display_name AS BLOB)) BETWEEN 1 AND 512),
    default_upstream_model TEXT COLLATE BINARY
        CHECK (default_upstream_model IS NULL OR
            (length(CAST(default_upstream_model AS BLOB)) BETWEEN 1 AND 512
             AND instr(default_upstream_model, char(0)) = 0)),
    status TEXT NOT NULL COLLATE BINARY
        CHECK (status IN ('active', 'archived')),
    note TEXT CHECK (note IS NULL OR
        (length(CAST(note AS BLOB)) <= 1024 AND instr(note, char(0)) = 0)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    revision INTEGER NOT NULL CHECK (revision > 0),
    UNIQUE (canonical_model COLLATE BINARY)
);

CREATE TABLE model_offering_bindings (
    id TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 192),
    model_profile_id TEXT NOT NULL COLLATE BINARY
        REFERENCES model_profiles(id) ON DELETE CASCADE,
    station_key_id TEXT COLLATE BINARY
        REFERENCES station_keys(id) ON DELETE RESTRICT,
    station_id TEXT COLLATE BINARY
        REFERENCES stations(id) ON DELETE RESTRICT,
    upstream_model TEXT NOT NULL COLLATE BINARY
        CHECK (length(CAST(upstream_model AS BLOB)) BETWEEN 1 AND 512
            AND instr(upstream_model, char(0)) = 0),
    source TEXT NOT NULL COLLATE BINARY
        CHECK (source IN ('manual', 'discovered', 'migrated')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    note TEXT CHECK (note IS NULL OR
        (length(CAST(note AS BLOB)) <= 1024 AND instr(note, char(0)) = 0)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
    revision INTEGER NOT NULL CHECK (revision > 0),
    CHECK ((station_key_id IS NOT NULL) <> (station_id IS NOT NULL))
);

CREATE UNIQUE INDEX idx_model_bindings_profile_key
    ON model_offering_bindings(model_profile_id, station_key_id)
    WHERE station_key_id IS NOT NULL;

CREATE UNIQUE INDEX idx_model_bindings_profile_station
    ON model_offering_bindings(model_profile_id, station_id)
    WHERE station_id IS NOT NULL;

CREATE INDEX idx_model_bindings_lookup
    ON model_offering_bindings(model_profile_id, enabled, station_key_id, station_id);

CREATE TABLE legacy_model_alias_migration_reviews (
    id TEXT PRIMARY KEY COLLATE BINARY
        CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 192),
    legacy_alias_id TEXT COLLATE BINARY
        CHECK (legacy_alias_id IS NULL OR
            (length(CAST(legacy_alias_id AS BLOB)) BETWEEN 1 AND 192
             AND instr(legacy_alias_id, char(0)) = 0)),
    requested_model TEXT COLLATE BINARY
        CHECK (requested_model IS NULL OR
            (length(CAST(requested_model AS BLOB)) BETWEEN 1 AND 512
             AND instr(requested_model, char(0)) = 0)),
    selected_target TEXT COLLATE BINARY
        CHECK (selected_target IS NULL OR
            (length(CAST(selected_target AS BLOB)) BETWEEN 1 AND 512
             AND instr(selected_target, char(0)) = 0)),
    discarded_target TEXT COLLATE BINARY
        CHECK (discarded_target IS NULL OR
            (length(CAST(discarded_target AS BLOB)) BETWEEN 1 AND 512
             AND instr(discarded_target, char(0)) = 0)),
    migration_status TEXT NOT NULL COLLATE BINARY
        CHECK (migration_status IN (
            'invalid', 'disabled_ignored', 'selected', 'discarded', 'duplicate_review'
        )),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

CREATE INDEX idx_legacy_model_alias_reviews_status
    ON legacy_model_alias_migration_reviews(migration_status, requested_model COLLATE BINARY);

-- A mapping history row is the durable audit anchor for a complete document.
-- The application service appends canonical documents after subsequent CAS
-- writes; this baseline row records the migration provenance without secrets.
CREATE TABLE model_mapping_document_history (
    revision INTEGER PRIMARY KEY CHECK (revision > 0),
    document_json TEXT NOT NULL CHECK (json_valid(document_json)),
    source TEXT NOT NULL COLLATE BINARY
        CHECK (source IN ('migration', 'user', 'file_sync', 'restore', 'system')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
);

-- Legacy aliases are converted once.  The earliest enabled row is the only
-- target enabled in the new exact rule; every additional or unusable row is
-- retained as an explicit review item instead of becoming an implicit chain.
WITH ranked AS (
    SELECT
        id, client_model, upstream_model, enabled, created_at,
        ROW_NUMBER() OVER (
            PARTITION BY trim(client_model)
            ORDER BY CASE WHEN enabled = 1 THEN 0 ELSE 1 END,
                     created_at ASC, id ASC
        ) AS row_number
    FROM model_aliases
), valid_first AS (
    SELECT id, client_model, upstream_model
    FROM ranked
    WHERE enabled = 1
      AND row_number = 1
      AND trim(client_model) <> ''
      AND trim(upstream_model) <> ''
      AND length(CAST(trim(client_model) AS BLOB)) <= 512
      AND length(CAST(trim(upstream_model) AS BLOB)) <= 512
      -- The generated rule/target IDs encode client_model as hex and are
      -- bounded to 192 bytes by the destination schema. Keep oversized
      -- legacy aliases in the review table instead of aborting migration.
      AND length(CAST(hex(trim(client_model)) AS BLOB)) <= 160
      AND instr(trim(client_model), char(0)) = 0
      AND instr(trim(client_model), char(10)) = 0
      AND instr(trim(client_model), char(13)) = 0
      AND instr(trim(upstream_model), char(0)) = 0
      AND instr(trim(upstream_model), char(10)) = 0
      AND instr(trim(upstream_model), char(13)) = 0
)
INSERT INTO model_mapping_rules (
    id, priority, enabled, matcher_kind, matcher_value,
    endpoint_conditions_json, stream_condition, tools_condition,
    vision_condition, reasoning_condition, action_kind, fallback_trigger,
    note, created_at_ms, updated_at_ms, revision
)
SELECT
    'legacy-model-alias-rule:' || hex(client_model),
    1, 1, 'exact', trim(client_model),
    '[]', 'any', 'any', 'any', 'any', 'map_fixed', NULL,
    'migrated from model_aliases', 0, 0, 1
FROM valid_first;

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
SELECT 'model_mapping_rule:' || id, 1, 0, 'baseline_snapshot'
FROM model_mapping_rules
WHERE 1
ON CONFLICT(scope) DO NOTHING;

WITH ranked AS (
    SELECT
        id, client_model, upstream_model, enabled, created_at,
        ROW_NUMBER() OVER (
            PARTITION BY trim(client_model)
            ORDER BY CASE WHEN enabled = 1 THEN 0 ELSE 1 END,
                     created_at ASC, id ASC
        ) AS row_number
    FROM model_aliases
), valid_first AS (
    SELECT id, client_model, upstream_model
    FROM ranked
    WHERE enabled = 1
      AND row_number = 1
      AND trim(client_model) <> ''
      AND trim(upstream_model) <> ''
      AND length(CAST(trim(client_model) AS BLOB)) <= 512
      AND length(CAST(trim(upstream_model) AS BLOB)) <= 512
      AND length(CAST(hex(trim(client_model)) AS BLOB)) <= 160
      AND instr(trim(client_model), char(0)) = 0
      AND instr(trim(client_model), char(10)) = 0
      AND instr(trim(client_model), char(13)) = 0
      AND instr(trim(upstream_model), char(0)) = 0
      AND instr(trim(upstream_model), char(10)) = 0
      AND instr(trim(upstream_model), char(13)) = 0
)
INSERT INTO model_mapping_rule_targets (
    id, rule_id, position, target_kind, literal_upstream_model, model_profile_id
)
SELECT
    'legacy-model-alias-target:' || hex(id),
    'legacy-model-alias-rule:' || hex(client_model),
    0, 'literal', trim(upstream_model), NULL
FROM valid_first;

WITH ranked AS (
    SELECT
        id, client_model, upstream_model, enabled, created_at,
        ROW_NUMBER() OVER (
            PARTITION BY trim(client_model)
            ORDER BY CASE WHEN enabled = 1 THEN 0 ELSE 1 END,
                     created_at ASC, id ASC
        ) AS row_number
    FROM model_aliases
), ranked_with_selected AS (
    SELECT ranked.*,
           MAX(CASE WHEN enabled = 1 AND row_number = 1
                    AND trim(upstream_model) <> ''
                    AND length(CAST(trim(upstream_model) AS BLOB)) <= 512
                    AND instr(trim(upstream_model), char(0)) = 0
                    AND instr(trim(upstream_model), char(10)) = 0
                    AND instr(trim(upstream_model), char(13)) = 0
                    THEN trim(upstream_model) END)
               OVER (PARTITION BY trim(client_model)) AS selected_target
    FROM ranked
)
INSERT INTO legacy_model_alias_migration_reviews (
    id, legacy_alias_id, requested_model, selected_target, discarded_target,
    migration_status, created_at_ms
)
SELECT
    'legacy-model-alias-review:' || hex(id),
    CASE WHEN length(CAST(id AS BLOB)) BETWEEN 1 AND 192
              AND instr(id, char(0)) = 0 THEN id END,
    CASE WHEN trim(client_model) <> ''
              AND length(CAST(trim(client_model) AS BLOB)) <= 512
              AND instr(trim(client_model), char(0)) = 0
              AND instr(trim(client_model), char(10)) = 0
              AND instr(trim(client_model), char(13)) = 0
         THEN trim(client_model) END,
    selected_target,
    CASE WHEN enabled = 1 AND row_number > 1
        AND trim(upstream_model) <> ''
        AND length(CAST(trim(upstream_model) AS BLOB)) <= 512
        AND instr(trim(upstream_model), char(0)) = 0
        AND instr(trim(upstream_model), char(10)) = 0
        AND instr(trim(upstream_model), char(13)) = 0
        THEN trim(upstream_model) END,
    CASE
        WHEN trim(client_model) = '' OR trim(upstream_model) = ''
          OR length(CAST(trim(client_model) AS BLOB)) > 512
          OR length(CAST(trim(upstream_model) AS BLOB)) > 512
          OR length(CAST(hex(trim(client_model)) AS BLOB)) > 160
          OR instr(trim(client_model), char(0)) > 0
          OR instr(trim(client_model), char(10)) > 0
          OR instr(trim(client_model), char(13)) > 0
          OR instr(trim(upstream_model), char(0)) > 0
          OR instr(trim(upstream_model), char(10)) > 0
          OR instr(trim(upstream_model), char(13)) > 0
            THEN 'invalid'
        WHEN enabled = 0 THEN 'disabled_ignored'
        WHEN row_number = 1 THEN 'selected'
        ELSE 'discarded'
    END,
    0
FROM ranked_with_selected
WHERE enabled = 0
   OR row_number > 1
   OR trim(client_model) = ''
   OR trim(upstream_model) = ''
   OR length(CAST(trim(client_model) AS BLOB)) > 512
   OR length(CAST(trim(upstream_model) AS BLOB)) > 512
   OR length(CAST(hex(trim(client_model)) AS BLOB)) > 160
   OR instr(trim(client_model), char(0)) > 0
   OR instr(trim(client_model), char(10)) > 0
   OR instr(trim(client_model), char(13)) > 0
   OR instr(trim(upstream_model), char(0)) > 0
   OR instr(trim(upstream_model), char(10)) > 0
   OR instr(trim(upstream_model), char(13)) > 0;

INSERT INTO model_mapping_document_history (
    revision, document_json, source, created_at_ms
)
SELECT
    1,
    json_object(
        'formatVersion', 1,
        'baseRevision', 1,
        'policy', json_object('unmatchedModelBehavior', 'preserve'),
        'rules', COALESCE((
            SELECT json_group_array(json_object(
                'id', r.id,
                'priority', r.priority,
                'enabled', CASE WHEN r.enabled = 1 THEN json('true') ELSE json('false') END,
                'matcher', json_object('kind', 'exact', 'model', r.matcher_value),
                'conditions', json_object(
                    'endpointKinds', json(r.endpoint_conditions_json),
                    'stream', r.stream_condition,
                    'tools', r.tools_condition,
                    'vision', r.vision_condition,
                    'reasoning', r.reasoning_condition
                ),
                'action', json_object(
                    'kind', 'map_fixed',
                    'target', json_object(
                        'kind', 'literal',
                        'upstreamModel', t.literal_upstream_model
                    )
                ),
                'note', r.note,
                'revision', r.revision
            ))
            FROM model_mapping_rules r
            JOIN model_mapping_rule_targets t ON t.rule_id = r.id AND t.position = 0
            WHERE r.matcher_kind = 'exact' AND r.action_kind = 'map_fixed'
        ), json('[]')),
        'profiles', json('[]'),
        'bindings', json('[]')
    ),
    'migration',
    0;

-- Transitional native-model identity bridge.  Existing capability rows retain
-- model_alias_revision as historical provenance.  New producers can populate
-- endpoint_kind and identity_version while the later capability cutover
-- replaces the old composite key without changing this migration.
ALTER TABLE routing_capability_model_observations
    ADD COLUMN endpoint_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (length(CAST(endpoint_kind AS BLOB)) BETWEEN 1 AND 64);
ALTER TABLE routing_capability_model_observations
    ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (length(CAST(protocol_kind AS BLOB)) BETWEEN 1 AND 64);
ALTER TABLE routing_capability_model_observations
    ADD COLUMN identity_version INTEGER NOT NULL DEFAULT 1 CHECK (identity_version >= 1);
ALTER TABLE routing_capability_model_observations
    ADD COLUMN model_mapping_revision INTEGER CHECK (model_mapping_revision IS NULL OR model_mapping_revision > 0);
ALTER TABLE routing_capability_model_observations
    ADD COLUMN model_resolution_fence TEXT
        CHECK (model_resolution_fence IS NULL OR
            length(CAST(model_resolution_fence AS BLOB)) BETWEEN 1 AND 128);
ALTER TABLE routing_capability_model_verdicts
    ADD COLUMN endpoint_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (length(CAST(endpoint_kind AS BLOB)) BETWEEN 1 AND 64);
ALTER TABLE routing_capability_model_verdicts
    ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (length(CAST(protocol_kind AS BLOB)) BETWEEN 1 AND 64);
ALTER TABLE routing_capability_model_verdicts
    ADD COLUMN identity_version INTEGER NOT NULL DEFAULT 1 CHECK (identity_version >= 1);
ALTER TABLE routing_capability_model_verdicts
    ADD COLUMN model_mapping_revision INTEGER CHECK (model_mapping_revision IS NULL OR model_mapping_revision > 0);
ALTER TABLE routing_capability_model_verdicts
    ADD COLUMN model_resolution_fence TEXT
        CHECK (model_resolution_fence IS NULL OR
            length(CAST(model_resolution_fence AS BLOB)) BETWEEN 1 AND 128);

CREATE INDEX idx_routing_capability_model_native_identity
    ON routing_capability_model_verdicts(
        station_key_id, resolved_model, endpoint_kind, protocol_kind,
        credential_revision, endpoint_revision, identity_version
    );

UPDATE persistence_schema_compatibility
SET schema_version = 43,
    updated_by_migration = 43,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 43;

CREATE TEMP TABLE persistence_v43_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 43)
);

INSERT INTO persistence_v43_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v43_schema_guard;
