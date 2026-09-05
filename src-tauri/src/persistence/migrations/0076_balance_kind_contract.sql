-- Classify balance snapshots without rewriting their amounts.  The kind is a
-- durable semantic boundary: current projections must not infer it from a
-- free-form source label.
ALTER TABLE balance_snapshots
ADD COLUMN balance_kind TEXT NOT NULL DEFAULT 'legacy_unknown'
    CHECK (balance_kind IN (
        'account_balance',
        'station_key_quota',
        'subscription_quota',
        'usage_summary',
        'legacy_derived_aggregate',
        'legacy_unknown'
    ));

UPDATE balance_snapshots
SET balance_kind = CASE
    WHEN source = 'station_key_balance_aggregate' THEN 'legacy_derived_aggregate'
    WHEN scope = 'station_key' AND station_key_id IS NOT NULL THEN 'station_key_quota'
    WHEN scope = 'subscription' AND station_key_id IS NULL THEN 'subscription_quota'
    WHEN scope = 'station' AND station_key_id IS NULL
         AND source IN (
             'sub2api_account_profile',
             'sub2api_user_profile',
             'sub2api_auth_me',
             'newapi_user_self',
             'station_balance'
         ) THEN 'account_balance'
    ELSE 'legacy_unknown'
END;

-- Historical aggregates remain queryable for diagnostics but are never
-- spendability evidence, even if an older build wrote a stronger authority.
UPDATE balance_snapshots
SET evidence_confidence = 'unknown',
    spendability_authority = 'advisory',
    evidence_profile_version = COALESCE(evidence_profile_version, 'legacy-balance-kind-v1')
WHERE balance_kind = 'legacy_derived_aggregate';

CREATE TRIGGER balance_snapshots_kind_scope_insert_guard
BEFORE INSERT ON balance_snapshots
WHEN NOT (
    (NEW.balance_kind = 'account_balance'
     AND NEW.station_key_id IS NULL
     AND NEW.scope IN ('station', 'station_account'))
    OR (NEW.balance_kind = 'station_key_quota'
        AND NEW.station_key_id IS NOT NULL
        AND NEW.scope = 'station_key')
    OR (NEW.balance_kind = 'subscription_quota'
        AND NEW.station_key_id IS NULL
        AND NEW.scope = 'subscription')
    OR (NEW.balance_kind = 'usage_summary'
        AND NEW.station_key_id IS NULL
        AND NEW.scope = 'station')
    OR NEW.balance_kind IN ('legacy_derived_aggregate', 'legacy_unknown')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid balance kind/scope ownership');
END;

CREATE TRIGGER balance_snapshots_kind_scope_update_guard
BEFORE UPDATE OF station_key_id, scope, balance_kind ON balance_snapshots
WHEN NOT (
    (NEW.balance_kind = 'account_balance'
     AND NEW.station_key_id IS NULL
     AND NEW.scope IN ('station', 'station_account'))
    OR (NEW.balance_kind = 'station_key_quota'
        AND NEW.station_key_id IS NOT NULL
        AND NEW.scope = 'station_key')
    OR (NEW.balance_kind = 'subscription_quota'
        AND NEW.station_key_id IS NULL
        AND NEW.scope = 'subscription')
    OR (NEW.balance_kind = 'usage_summary'
        AND NEW.station_key_id IS NULL
        AND NEW.scope = 'station')
    OR NEW.balance_kind IN ('legacy_derived_aggregate', 'legacy_unknown')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid balance kind/scope ownership');
END;

CREATE INDEX idx_balance_snapshots_current_kind
    ON balance_snapshots(station_id, balance_kind, scope, station_key_id,
                         updated_at DESC, created_at DESC, id DESC);

UPDATE persistence_schema_compatibility
SET schema_version = 76,
    updated_by_migration = 76,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1 AND schema_version < 76;

CREATE TEMP TABLE persistence_v76_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 76)
);

INSERT INTO persistence_v76_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v76_schema_guard;
