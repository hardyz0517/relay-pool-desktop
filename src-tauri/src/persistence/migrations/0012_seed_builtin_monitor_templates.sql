-- Status Monitoring V2 owns request construction through protocol adapters and
-- Profiles. These rows preserve the legacy template foreign key without
-- requiring users to manage compatibility data.
INSERT INTO channel_monitor_request_templates (
    id,
    name,
    endpoint_kind,
    method,
    path,
    request_body_json,
    enabled,
    built_in,
    note,
    created_at,
    updated_at
) VALUES
    (
        'builtin-openai-chat-low-token',
        'OpenAI Chat low-token probe',
        'chat_completions',
        'POST',
        '/v1/chat/completions',
        '{"messages":[{"role":"user","content":"Reply ok."}],"max_tokens":1,"stream":false}',
        1,
        1,
        'System compatibility template for Status Monitoring V2.',
        strftime('%s', 'now') || '000',
        strftime('%s', 'now') || '000'
    ),
    (
        'builtin-openai-responses-low-token',
        'OpenAI Responses low-token probe',
        'responses',
        'POST',
        '/v1/responses',
        '{"input":"Reply ok.","max_output_tokens":1,"stream":false}',
        1,
        1,
        'System compatibility template for Status Monitoring V2.',
        strftime('%s', 'now') || '000',
        strftime('%s', 'now') || '000'
    )
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    endpoint_kind = excluded.endpoint_kind,
    method = excluded.method,
    path = excluded.path,
    request_body_json = excluded.request_body_json,
    enabled = 1,
    built_in = 1,
    note = excluded.note,
    updated_at = excluded.updated_at;

UPDATE persistence_schema_compatibility
SET schema_version = 12,
    updated_by_migration = 12,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE singleton_key = 1
  AND schema_version = 11;

CREATE TEMP TABLE persistence_v12_schema_guard (
    schema_version INTEGER NOT NULL CHECK (schema_version = 12)
);

INSERT INTO persistence_v12_schema_guard (schema_version)
SELECT schema_version
FROM persistence_schema_compatibility
WHERE singleton_key = 1;

DROP TABLE persistence_v12_schema_guard;
