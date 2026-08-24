-- Materialize routing-policy durations in seconds.
--
-- Policy revisions are intentionally left untouched: this is a storage
-- projection of existing semantics, not a user edit. Only complete legacy
-- nested objects are converted. Partial, mixed, or wrong-typed documents stay
-- unchanged so the typed decoder can surface them as recoverable errors.

UPDATE routing_policy
SET config_json = json_set(
        config_json,
        '$.retryFailover',
        CASE
            WHEN json_type(config_json, '$.retryFailover') = 'object'
             AND json_extract(config_json, '$.retryFailover.version') = 1
             AND json_type(config_json, '$.retryFailover.maxTotalAttempts') = 'integer'
             AND json_type(config_json, '$.retryFailover.maxSameTargetCapacityRetries') = 'integer'
             AND json_type(config_json, '$.retryFailover.capacityRetryWaitBudgetMs') IN ('integer', 'real')
             AND json_type(config_json, '$.retryFailover.capacityRetryWaitBudgetSeconds') IS NULL
             AND json_type(config_json, '$.retryFailover.allowCrossCapacityDomainFallback') IN ('true', 'false')
                THEN json_object(
                    'version', 2,
                    'maxTotalAttempts', json_extract(config_json, '$.retryFailover.maxTotalAttempts'),
                    'maxSameTargetCapacityRetries', json_extract(config_json, '$.retryFailover.maxSameTargetCapacityRetries'),
                    'capacityRetryWaitBudgetSeconds', json_extract(config_json, '$.retryFailover.capacityRetryWaitBudgetMs') / 1000.0,
                    'allowCrossCapacityDomainFallback', CASE
                        WHEN json_extract(config_json, '$.retryFailover.allowCrossCapacityDomainFallback') = 1
                            THEN json('true') ELSE json('false') END
                )
            ELSE json_extract(config_json, '$.retryFailover')
        END,
        '$.protectionProfile',
        CASE
            WHEN json_type(config_json, '$.protectionProfile') = 'object'
             AND json_extract(config_json, '$.protectionProfile.version') = 1
             AND json_type(config_json, '$.protectionProfile.enabled') IN ('true', 'false')
             AND json_type(config_json, '$.protectionProfile.windowMaxSamples') = 'integer'
             AND json_type(config_json, '$.protectionProfile.windowMs') IN ('integer', 'real')
             AND json_type(config_json, '$.protectionProfile.windowSeconds') IS NULL
             AND json_type(config_json, '$.protectionProfile.minSamples') = 'integer'
             AND json_type(config_json, '$.protectionProfile.failureThresholdPercent') = 'integer'
             AND json_type(config_json, '$.protectionProfile.halfOpenSuccessesToClose') = 'integer'
                THEN json_object(
                    'version', 2,
                    'enabled', CASE
                        WHEN json_extract(config_json, '$.protectionProfile.enabled') = 1
                            THEN json('true') ELSE json('false') END,
                    'windowMaxSamples', json_extract(config_json, '$.protectionProfile.windowMaxSamples'),
                    'windowSeconds', json_extract(config_json, '$.protectionProfile.windowMs') / 1000.0,
                    'minSamples', json_extract(config_json, '$.protectionProfile.minSamples'),
                    'failureThresholdPercent', json_extract(config_json, '$.protectionProfile.failureThresholdPercent'),
                    'halfOpenSuccessesToClose', json_extract(config_json, '$.protectionProfile.halfOpenSuccessesToClose')
                )
            ELSE json_extract(config_json, '$.protectionProfile')
        END,
        '$.timeoutPolicy',
        CASE
            WHEN json_type(config_json, '$.timeoutPolicy') = 'object'
             AND json_extract(config_json, '$.timeoutPolicy.version') = 1
             AND json_type(config_json, '$.timeoutPolicy.connectMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.firstByteMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.precommitMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.bufferedExecutionMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.streamIdleMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.connectSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.firstByteSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.precommitSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.bufferedExecutionSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.streamIdleSeconds') IS NULL
                THEN json_object(
                    'version', 2,
                    'connectSeconds', json_extract(config_json, '$.timeoutPolicy.connectMs') / 1000.0,
                    'firstByteSeconds', json_extract(config_json, '$.timeoutPolicy.firstByteMs') / 1000.0,
                    'precommitSeconds', json_extract(config_json, '$.timeoutPolicy.precommitMs') / 1000.0,
                    'bufferedExecutionSeconds', json_extract(config_json, '$.timeoutPolicy.bufferedExecutionMs') / 1000.0,
                    'streamIdleSeconds', json_extract(config_json, '$.timeoutPolicy.streamIdleMs') / 1000.0
                )
            ELSE json_extract(config_json, '$.timeoutPolicy')
        END
    )
WHERE singleton_key = 1
  AND json_extract(config_json, '$.version') = 2
  AND (
      json_extract(config_json, '$.retryFailover.version') = 1
      OR json_extract(config_json, '$.protectionProfile.version') = 1
      OR json_extract(config_json, '$.timeoutPolicy.version') = 1
  );

UPDATE routing_policy_history
SET config_json = json_set(
        config_json,
        '$.retryFailover',
        CASE
            WHEN json_type(config_json, '$.retryFailover') = 'object'
             AND json_extract(config_json, '$.retryFailover.version') = 1
             AND json_type(config_json, '$.retryFailover.maxTotalAttempts') = 'integer'
             AND json_type(config_json, '$.retryFailover.maxSameTargetCapacityRetries') = 'integer'
             AND json_type(config_json, '$.retryFailover.capacityRetryWaitBudgetMs') IN ('integer', 'real')
             AND json_type(config_json, '$.retryFailover.capacityRetryWaitBudgetSeconds') IS NULL
             AND json_type(config_json, '$.retryFailover.allowCrossCapacityDomainFallback') IN ('true', 'false')
                THEN json_object(
                    'version', 2,
                    'maxTotalAttempts', json_extract(config_json, '$.retryFailover.maxTotalAttempts'),
                    'maxSameTargetCapacityRetries', json_extract(config_json, '$.retryFailover.maxSameTargetCapacityRetries'),
                    'capacityRetryWaitBudgetSeconds', json_extract(config_json, '$.retryFailover.capacityRetryWaitBudgetMs') / 1000.0,
                    'allowCrossCapacityDomainFallback', CASE
                        WHEN json_extract(config_json, '$.retryFailover.allowCrossCapacityDomainFallback') = 1
                            THEN json('true') ELSE json('false') END
                )
            ELSE json_extract(config_json, '$.retryFailover')
        END,
        '$.protectionProfile',
        CASE
            WHEN json_type(config_json, '$.protectionProfile') = 'object'
             AND json_extract(config_json, '$.protectionProfile.version') = 1
             AND json_type(config_json, '$.protectionProfile.enabled') IN ('true', 'false')
             AND json_type(config_json, '$.protectionProfile.windowMaxSamples') = 'integer'
             AND json_type(config_json, '$.protectionProfile.windowMs') IN ('integer', 'real')
             AND json_type(config_json, '$.protectionProfile.windowSeconds') IS NULL
             AND json_type(config_json, '$.protectionProfile.minSamples') = 'integer'
             AND json_type(config_json, '$.protectionProfile.failureThresholdPercent') = 'integer'
             AND json_type(config_json, '$.protectionProfile.halfOpenSuccessesToClose') = 'integer'
                THEN json_object(
                    'version', 2,
                    'enabled', CASE
                        WHEN json_extract(config_json, '$.protectionProfile.enabled') = 1
                            THEN json('true') ELSE json('false') END,
                    'windowMaxSamples', json_extract(config_json, '$.protectionProfile.windowMaxSamples'),
                    'windowSeconds', json_extract(config_json, '$.protectionProfile.windowMs') / 1000.0,
                    'minSamples', json_extract(config_json, '$.protectionProfile.minSamples'),
                    'failureThresholdPercent', json_extract(config_json, '$.protectionProfile.failureThresholdPercent'),
                    'halfOpenSuccessesToClose', json_extract(config_json, '$.protectionProfile.halfOpenSuccessesToClose')
                )
            ELSE json_extract(config_json, '$.protectionProfile')
        END,
        '$.timeoutPolicy',
        CASE
            WHEN json_type(config_json, '$.timeoutPolicy') = 'object'
             AND json_extract(config_json, '$.timeoutPolicy.version') = 1
             AND json_type(config_json, '$.timeoutPolicy.connectMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.firstByteMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.precommitMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.bufferedExecutionMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.streamIdleMs') IN ('integer', 'real')
             AND json_type(config_json, '$.timeoutPolicy.connectSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.firstByteSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.precommitSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.bufferedExecutionSeconds') IS NULL
             AND json_type(config_json, '$.timeoutPolicy.streamIdleSeconds') IS NULL
                THEN json_object(
                    'version', 2,
                    'connectSeconds', json_extract(config_json, '$.timeoutPolicy.connectMs') / 1000.0,
                    'firstByteSeconds', json_extract(config_json, '$.timeoutPolicy.firstByteMs') / 1000.0,
                    'precommitSeconds', json_extract(config_json, '$.timeoutPolicy.precommitMs') / 1000.0,
                    'bufferedExecutionSeconds', json_extract(config_json, '$.timeoutPolicy.bufferedExecutionMs') / 1000.0,
                    'streamIdleSeconds', json_extract(config_json, '$.timeoutPolicy.streamIdleMs') / 1000.0
                )
            ELSE json_extract(config_json, '$.timeoutPolicy')
        END
    )
WHERE json_extract(config_json, '$.version') = 2
  AND (
      json_extract(config_json, '$.retryFailover.version') = 1
      OR json_extract(config_json, '$.protectionProfile.version') = 1
      OR json_extract(config_json, '$.timeoutPolicy.version') = 1
  );

UPDATE persistence_schema_compatibility
SET schema_version = 54,
    updated_by_migration = 54,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 54;

CREATE TEMP TABLE persistence_v54_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 54)
);
INSERT INTO persistence_v54_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;
DROP TABLE persistence_v54_schema_guard;
