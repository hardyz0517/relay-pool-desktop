-- Materialize the routing-policy aggregate to the V2 storage shape.
--
-- V1 rows remain a supported decoder input for damaged/partial historical
-- data, but normal active and history rows are rewritten additively so the
-- runtime does not need to keep a V1-shaped active row indefinitely.  The
-- routing revision is deliberately unchanged: this is a schema projection,
-- not a policy edit, and in-flight request snapshots must remain valid.
-- Optional V1 fields retain their documented compatibility defaults. Required
-- V1 fields are never defaulted; a malformed row therefore remains visible to
-- the typed storage decoder as a recoverable invalid input.

UPDATE routing_policy
SET config_json = json_object(
        'version', 2,
        'reliabilityWeight', json_extract(config_json, '$.reliability_weight'),
        'responsivenessWeight', json_extract(config_json, '$.responsiveness_weight'),
        'costWeight', json_extract(config_json, '$.cost_weight'),
        'preferenceWeight', json_extract(config_json, '$.preference_weight'),
        'maxCandidates', json_extract(config_json, '$.max_candidates'),
        'explorationShareBasisPoints', json_extract(config_json, '$.exploration_share_basis_points'),
        'allowDepletedFallback', CASE
            WHEN json_extract(config_json, '$.allow_depleted_fallback') = 1
                THEN json('true') ELSE json('false') END,
        'affinityEnabled', CASE
            WHEN json_extract(config_json, '$.affinity_enabled') = 1
                THEN json('true') ELSE json('false') END,
        'affinityTtlSeconds', json_extract(config_json, '$.affinity_ttl_seconds'),
        'maxRateMultiplier', json_extract(config_json, '$.max_rate_multiplier'),
        'routingGroupFilter', json(
            CASE
                WHEN json_type(config_json, '$.routing_group_filter') IN ('object', 'array')
                    THEN json_extract(config_json, '$.routing_group_filter')
                ELSE json_quote(COALESCE(
                    json_extract(config_json, '$.routing_group_filter'), 'all_groups'))
            END
        ),
        'outboundProxyMode', COALESCE(
            json_extract(config_json, '$.outbound_proxy_mode'), 'inherit'),
        'outboundProxyUrl', json_extract(config_json, '$.outbound_proxy_url'),
        'retryFailover', json_object(
            'version', 1,
            'maxTotalAttempts', 4,
            'maxSameTargetCapacityRetries', 2,
            'capacityRetryWaitBudgetMs', 2000,
            'allowCrossCapacityDomainFallback', json('true')
        ),
        'protectionProfile', json_object(
            'version', 1,
            'enabled', json('false'),
            'windowMaxSamples', 64,
            'windowMs', 300000,
            'minSamples', 5,
            'failureThresholdPercent', 60,
            'halfOpenSuccessesToClose', 2
        )
    ),
    policy_version = 'routing-policy-v2'
WHERE singleton_key = 1
  AND json_extract(config_json, '$.version') = 1
  AND json_extract(config_json, '$.reliability_weight') IS NOT NULL
  AND json_extract(config_json, '$.responsiveness_weight') IS NOT NULL
  AND json_extract(config_json, '$.cost_weight') IS NOT NULL
  AND json_extract(config_json, '$.preference_weight') IS NOT NULL
  AND json_extract(config_json, '$.max_candidates') IS NOT NULL
  AND json_extract(config_json, '$.exploration_share_basis_points') IS NOT NULL
  AND json_type(config_json, '$.allow_depleted_fallback') IN ('true', 'false')
  AND json_type(config_json, '$.affinity_enabled') IN ('true', 'false')
  AND json_extract(config_json, '$.affinity_ttl_seconds') IS NOT NULL;

UPDATE routing_policy_history
SET config_json = json_object(
        'version', 2,
        'reliabilityWeight', json_extract(config_json, '$.reliability_weight'),
        'responsivenessWeight', json_extract(config_json, '$.responsiveness_weight'),
        'costWeight', json_extract(config_json, '$.cost_weight'),
        'preferenceWeight', json_extract(config_json, '$.preference_weight'),
        'maxCandidates', json_extract(config_json, '$.max_candidates'),
        'explorationShareBasisPoints', json_extract(config_json, '$.exploration_share_basis_points'),
        'allowDepletedFallback', CASE
            WHEN json_extract(config_json, '$.allow_depleted_fallback') = 1
                THEN json('true') ELSE json('false') END,
        'affinityEnabled', CASE
            WHEN json_extract(config_json, '$.affinity_enabled') = 1
                THEN json('true') ELSE json('false') END,
        'affinityTtlSeconds', json_extract(config_json, '$.affinity_ttl_seconds'),
        'maxRateMultiplier', json_extract(config_json, '$.max_rate_multiplier'),
        'routingGroupFilter', json(
            CASE
                WHEN json_type(config_json, '$.routing_group_filter') IN ('object', 'array')
                    THEN json_extract(config_json, '$.routing_group_filter')
                ELSE json_quote(COALESCE(
                    json_extract(config_json, '$.routing_group_filter'), 'all_groups'))
            END
        ),
        'outboundProxyMode', COALESCE(
            json_extract(config_json, '$.outbound_proxy_mode'), 'inherit'),
        'outboundProxyUrl', json_extract(config_json, '$.outbound_proxy_url'),
        'retryFailover', json_object(
            'version', 1,
            'maxTotalAttempts', 4,
            'maxSameTargetCapacityRetries', 2,
            'capacityRetryWaitBudgetMs', 2000,
            'allowCrossCapacityDomainFallback', json('true')
        ),
        'protectionProfile', json_object(
            'version', 1,
            'enabled', json('false'),
            'windowMaxSamples', 64,
            'windowMs', 300000,
            'minSamples', 5,
            'failureThresholdPercent', 60,
            'halfOpenSuccessesToClose', 2
        )
    ),
    policy_version = 'routing-policy-v2'
WHERE json_extract(config_json, '$.version') = 1
  AND json_extract(config_json, '$.reliability_weight') IS NOT NULL
  AND json_extract(config_json, '$.responsiveness_weight') IS NOT NULL
  AND json_extract(config_json, '$.cost_weight') IS NOT NULL
  AND json_extract(config_json, '$.preference_weight') IS NOT NULL
  AND json_extract(config_json, '$.max_candidates') IS NOT NULL
  AND json_extract(config_json, '$.exploration_share_basis_points') IS NOT NULL
  AND json_type(config_json, '$.allow_depleted_fallback') IN ('true', 'false')
  AND json_type(config_json, '$.affinity_enabled') IN ('true', 'false')
  AND json_extract(config_json, '$.affinity_ttl_seconds') IS NOT NULL;

UPDATE persistence_schema_compatibility
SET schema_version = 50,
    updated_by_migration = 50,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 49;

CREATE TEMP TABLE persistence_v50_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 50)
);
INSERT INTO persistence_v50_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v50_schema_guard;
