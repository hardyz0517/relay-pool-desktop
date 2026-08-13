-- Routing limits and group selection are policy inputs. Move their legacy
-- Settings values into the versioned policy aggregate exactly once.
UPDATE routing_policy
SET config_json = json_set(
    json_set(
        config_json,
        '$.max_rate_multiplier',
        json(COALESCE((
            SELECT CASE
                WHEN trim(value) = '' THEN 'null'
                WHEN json_valid(value) THEN value
                ELSE 'null'
            END
            FROM settings
            WHERE key = 'max_rate_multiplier'
        ), 'null'))
    ),
    '$.routing_group_filter',
    json(COALESCE((
        SELECT CASE
            WHEN json_valid(value) THEN value
            ELSE json_quote(value)
        END
        FROM settings
        WHERE key = 'default_routing_group_filter'
    ), '"all_groups"'))
)
WHERE singleton_key = 1
  AND (json_type(config_json, '$.max_rate_multiplier') IS NULL
       OR json_type(config_json, '$.routing_group_filter') IS NULL);

UPDATE persistence_schema_compatibility
SET schema_version = 36,
    updated_by_migration = 36,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version < 36;
