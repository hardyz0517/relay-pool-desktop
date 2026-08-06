-- Ensure databases that already applied the additive foundation receive an
-- explicit canonical V1 policy before the proxy cutover.
INSERT INTO routing_policy (
    singleton_key, config_json, config_revision, policy_version,
    system_version, status, created_at_ms, updated_at_ms
)
SELECT
    1,
    '{"version":1,"reliability_weight":4000,"responsiveness_weight":2500,"cost_weight":2000,"preference_weight":1500,"max_candidates":64,"exploration_share_basis_points":500,"allow_depleted_fallback":false,"affinity_enabled":false,"affinity_ttl_seconds":300}',
    1,
    'routing_policy_v1',
    'intelligent-routing-engine',
    'active',
    0,
    0
WHERE NOT EXISTS (SELECT 1 FROM routing_policy WHERE singleton_key = 1);

INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
VALUES ('routing_policy', 1, 0, 'baseline_snapshot')
ON CONFLICT(scope) DO NOTHING;

UPDATE persistence_schema_compatibility
SET schema_version = 25,
    updated_by_migration = 25,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 25;
