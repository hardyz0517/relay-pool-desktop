-- Correct the baseline priority assigned to legacy aliases in migration 43.
-- Migration 43 is already part of the immutable history; keep this repair in
-- a new append-only migration so SQLx checksum validation remains meaningful.
UPDATE model_mapping_rules
SET priority = 1
WHERE substr(id, 1, length('legacy-model-alias-rule:')) = 'legacy-model-alias-rule:'
  AND note = 'migrated from model_aliases'
  AND priority = 0;

-- Revision 1 is the migration provenance document. Rebuild only that row from
-- the normalized projection so restore/replay cannot reintroduce priority 0.
UPDATE model_mapping_document_history
SET document_json = json_object(
        'formatVersion', 1,
        'baseRevision', 1,
        'policy', json_object(
            'unmatchedModelBehavior',
            (SELECT unmatched_model_behavior
             FROM model_mapping_policies
             WHERE singleton_key = 1)
        ),
        'rules', COALESCE((
            SELECT json_group_array(json_object(
                'id', r.id,
                'priority', r.priority,
                'enabled', CASE WHEN r.enabled = 1 THEN json('true') ELSE json('false') END,
                'matcher', json_object('kind', r.matcher_kind, 'model', r.matcher_value),
                'conditions', json_object(
                    'endpointKinds', json(r.endpoint_conditions_json),
                    'stream', r.stream_condition,
                    'tools', r.tools_condition,
                    'vision', r.vision_condition,
                    'reasoning', r.reasoning_condition
                ),
                'action', json_object(
                    'kind', r.action_kind,
                    'target', CASE
                        WHEN r.action_kind = 'map_fixed' THEN json_object(
                            'kind', 'literal',
                            'upstreamModel', t.literal_upstream_model
                        )
                        ELSE json_object()
                    END
                ),
                'note', r.note,
                'revision', r.revision
            ))
            FROM model_mapping_rules r
            LEFT JOIN model_mapping_rule_targets t
              ON t.rule_id = r.id AND t.position = 0
            WHERE r.matcher_kind = 'exact'
        ), json('[]')),
        'profiles', json('[]'),
        'bindings', json('[]')
    )
WHERE revision = 1
  AND source = 'migration';

UPDATE persistence_schema_compatibility
SET schema_version = 47,
    updated_by_migration = 47,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 47;

CREATE TEMP TABLE persistence_v47_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 47)
);

INSERT INTO persistence_v47_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v47_schema_guard;
