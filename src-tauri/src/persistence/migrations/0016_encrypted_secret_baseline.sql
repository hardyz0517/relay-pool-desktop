ALTER TABLE secrets ADD COLUMN key_id TEXT;
ALTER TABLE secrets ADD COLUMN encryption_version INTEGER;
ALTER TABLE secrets ADD COLUMN value_hash TEXT NOT NULL DEFAULT '';

CREATE TABLE app_secret_bindings (
    binding_scope TEXT NOT NULL,
    binding_owner_id TEXT NOT NULL,
    binding_kind TEXT NOT NULL,
    secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (binding_scope, binding_owner_id, binding_kind),
    UNIQUE(secret_id)
);

CREATE INDEX idx_app_secret_bindings_secret_id ON app_secret_bindings(secret_id);

-- Intentionally do not update persistence_schema_compatibility here.
-- Application-level baseline conversion must re-encrypt and validate all
-- secret-bearing rows before committing schema_version = 16.
