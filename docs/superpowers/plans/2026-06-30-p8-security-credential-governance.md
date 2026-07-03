# P8 Security and Credential Governance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Relay Pool Desktop safe enough for long-term personal use with real Station Keys, station login credentials, collector snapshots, request logs, and local proxy traffic by encrypting stored secrets and enforcing one redaction boundary.

**Architecture:** P8 introduces a backend `SecretManager` that becomes the only code path for storing, reading, updating, deleting, masking, and redacting sensitive values. SQLite keeps route-readable metadata, masked values, and encrypted secret references; the encryption key is protected by the host OS keychain, while all logs, snapshots, UI surfaces, import/export paths, and proxy errors pass through shared redaction utilities.

**Tech Stack:** Tauri 2, Rust, rusqlite, serde, serde_json, `keyring`, `aes-gcm`, `rand`, `base64`, React, TypeScript, Vite, existing proxy/router/collector/database modules.

---

## Product Boundary

P8 is security and credential governance. It does not expand routing, pricing, collection, or proxy feature scope except where those modules must call the shared secret and redaction boundary.

P8 protects:

- Station Key API keys.
- Legacy station `api_key` values.
- Station login passwords.
- Future token, cookie, session, and authorization values.
- Captured WebView event payloads.
- Collector snapshot raw and normalized JSON.
- Request log error messages.
- Route details and rejected candidate JSON.
- Proxy upstream errors.
- Import, export, and backup data.
- UI fields that display or copy sensitive values.

P8 does not build:

- new pricing route strategies
- new collector adapters
- new proxy endpoints
- cloud sync
- team permissions
- enterprise audit logs
- a full password manager
- a public LAN proxy mode

---

## Recommended Security Design

### Storage Strategy

Use envelope encryption:

1. Generate a 256-bit app data encryption key.
2. Store that key in the host OS keychain through the `keyring` crate.
3. Store encrypted secret payloads in SQLite using AES-256-GCM.
4. Store only metadata, masked previews, and secret references in business tables.

This is the recommended P8 path because:

- the app already uses SQLite heavily;
- routing needs to load Station Key secrets quickly;
- keychain storage per secret would complicate listing, backup, and migration;
- pure app-level static encryption would not protect the database if copied from disk;
- a user-entered master password adds friction and recovery problems that are larger than P8 needs.

### SQLite Secret Shape

Create a `secrets` table that owns encrypted values:

```sql
CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    ciphertext TEXT NOT NULL,
    nonce TEXT NOT NULL,
    aad TEXT NOT NULL,
    masked_value TEXT NOT NULL,
    value_hash TEXT NOT NULL,
    encryption_version INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_secrets_owner_kind
ON secrets(owner_id, kind);
```

Rules:

- `scope` is one of `station`, `station_key`, `collector`, `proxy`, `settings`.
- `kind` is one of `api_key`, `login_password`, `token`, `cookie`, `session`, `authorization`.
- `ciphertext`, `nonce`, and `aad` are base64 strings.
- `masked_value` is safe for UI display.
- `value_hash` is a SHA-256 digest for change detection and tests; it is not shown in UI.
- `aad` includes `scope`, `owner_id`, and `kind` so ciphertext cannot be moved between records.

### Business Table Migration

Add references while keeping old columns temporarily for safe migration:

```sql
ALTER TABLE station_keys ADD COLUMN api_key_secret_id TEXT;
ALTER TABLE stations ADD COLUMN api_key_secret_id TEXT;
ALTER TABLE station_credentials ADD COLUMN login_password_secret_id TEXT;
```

Migration rules:

- Existing plaintext values are encrypted into `secrets`.
- New reads prefer `*_secret_id`.
- Legacy plaintext is only used as a migration source.
- Once a secret row is verified, plaintext columns are set to `NULL`.
- If migration fails for a row, keep the original plaintext, record a migration error in `secret_migration_events`, and continue with other rows.

---

## Completion Standard

P8 is complete only when all gates below pass.

### Functional Gates

- [ ] Station Key creation and editing stores the full API key only in `secrets`.
- [ ] Local proxy can still route real requests by resolving Station Key secrets through `SecretManager`.
- [ ] Station login password saving stores the password only in `secrets`.
- [ ] Existing plaintext station keys, legacy station `api_key`, and login passwords migrate without data loss.
- [ ] UI pages only receive `apiKeyMasked`, `apiKeyPresent`, `passwordPresent`, and secret metadata by default.
- [ ] Any explicit reveal/copy command is backend-mediated and never returns a secret through list APIs.
- [ ] Collector snapshots are redacted before persistence.
- [ ] Request logs and route details are redacted before persistence.
- [ ] Proxy errors returned to clients do not include full keys, cookies, tokens, local database paths, or internal Rust details.
- [ ] `.db`, `.db-wal`, `.db-shm`, `.log`, `.env`, `.codegraph/`, token/session/cookie files remain ignored by git.

### Security Gates

- [ ] A raw SQLite scan cannot find a known test key such as `sk-p8-secret-plaintext-canary`.
- [ ] A raw SQLite scan cannot find a known test password such as `p8-password-canary`.
- [ ] A raw SQLite scan cannot find a known cookie such as `rpd_session=p8-cookie-canary`.
- [ ] `request_logs.error_message` contains redacted summaries only.
- [ ] `request_logs.rejected_candidates_json` contains no `api_key`, `Authorization`, `Bearer`, cookie, token, or password values.
- [ ] `collector_snapshots.raw_json_redacted` contains no raw key, password, cookie, token, authorization header, prompt, or full response body.
- [ ] The proxy still binds only to `127.0.0.1`.
- [ ] CORS is documented and limited to local OpenAI-compatible use; management commands remain Tauri-only.

### Verification Gates

- [ ] `pnpm build`
- [ ] `cargo check --manifest-path .\src-tauri\Cargo.toml`
- [ ] `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
- [ ] Manual smoke: create a Station Key, restart app, proxy request still works.
- [ ] Manual smoke: edit a Station Key with blank API key field, old encrypted key remains usable.
- [ ] Manual smoke: save station login password, restart app, login test can still use it.
- [ ] Manual smoke: request logs show metadata without prompt/response/key leakage.
- [ ] Manual smoke: collector snapshot developer JSON is redacted.
- [ ] Manual smoke: local proxy port is not reachable through `0.0.0.0` or LAN address.

---

## File Structure

### Backend Secret Layer

- Create `src-tauri/src/models/secrets.rs`
  - `SecretKind`, `SecretScope`, `SecretRecord`, `SecretRef`, `CreateSecretInput`, `UpdateSecretInput`, `SecretMigrationReport`, `SecretScanFinding`.
- Create `src-tauri/src/services/secrets/mod.rs`
  - module exports and `SecretManager`.
- Create `src-tauri/src/services/secrets/crypto.rs`
  - AES-GCM encrypt/decrypt helpers.
- Create `src-tauri/src/services/secrets/keychain.rs`
  - OS keychain master-key loading and creation.
- Create `src-tauri/src/services/secrets/mask.rs`
  - shared masking and redaction functions.
- Create `src-tauri/src/services/secrets/audit.rs`
  - SQLite canary scan helpers and migration report helpers.
- Modify `src-tauri/src/models/mod.rs`
  - export `secrets`.
- Modify `src-tauri/src/services/mod.rs`
  - export `secrets`.
- Modify `src-tauri/src/lib.rs`
  - initialize and manage `SecretManager`.

### Backend Database

- Modify `src-tauri/src/services/database.rs`
  - create `secrets` and `secret_migration_events`;
  - add secret reference columns;
  - migrate plaintext values;
  - update station key and credentials read/write flows;
  - add secret scan tests.

### Backend Proxy, Routing, Collector, Capture

- Modify `src-tauri/src/services/proxy/mod.rs`
  - keep `RouteCandidate.api_key` runtime-only; never serialize it.
- Modify `src-tauri/src/services/proxy/runtime.rs`
  - resolve secret at request time through `SecretManager`;
  - redact all errors before logs and client responses;
  - add CORS/local-bind security tests.
- Modify `src-tauri/src/services/proxy/router.rs`
  - ensure route explanations never include raw API key material.
- Modify `src-tauri/src/services/capture/redaction.rs`
  - move or delegate to shared `services::secrets::mask`.
- Modify `src-tauri/src/services/capture/mod.rs`
  - use shared redaction on captured event fields.
- Modify `src-tauri/src/services/collectors/mod.rs`
  - redact collector errors and snapshots before persistence.
- Modify `src-tauri/src/services/collectors/sub2api.rs`
  - use shared redaction on upstream errors and raw JSON fields.
- Modify `src-tauri/src/services/logs/mod.rs`
  - use shared redaction on derived log summaries.

### Backend Commands

- Modify `src-tauri/src/commands/mod.rs`
  - pass `SecretManager` into station key, credential, proxy, and collector commands that need secret resolution;
  - add security audit commands:
    - `get_secret_migration_status`
    - `run_secret_safety_scan`
    - `reveal_station_key_once`
    - `copy_station_key_to_clipboard` only if implemented through an explicit UI action.

### Frontend API and Types

- Create `src/lib/types/secrets.ts`
  - migration status, scan finding, reveal response.
- Create `src/lib/api/secrets.ts`
  - Tauri invoke wrappers for security status and explicit reveal/copy actions.
- Modify `src/lib/types/stationKeys.ts`
  - keep input fields for user entry; list/read types must remain masked-only.
- Modify `src/lib/types/stations.ts`
  - keep station outputs masked-only.
- Modify `src/lib/types/proxy.ts`
  - request log types remain metadata-only.

### Frontend UI

- Modify `src/components/ui/MaskedSecret.tsx`
  - upgrade to reusable masked display with optional reveal/copy affordance.
- Modify `src/features/key-pool/KeyPoolPage.tsx`
  - display masked key through `MaskedSecret`; edit dialog keeps blank API key as preserve-old-secret.
- Modify `src/features/stations/StationsPage.tsx`
  - station key and login credential display uses `MaskedSecret` or presence labels only.
- Modify `src/features/collectors/CollectorsPage.tsx`
  - developer JSON copy uses backend-redacted snapshot only.
- Modify `src/features/logs/LogsPage.tsx`
  - never render raw prompt/response/key; render redacted route details.
- Modify `src/features/routing/RoutingPage.tsx`
  - route simulator displays masked key identity and redacted rejection reasons.
- Modify `src/features/settings/SettingsPage.tsx`
  - add a compact Security section showing encryption and migration status.

### Documentation

- Create `docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md`
  - product-facing phase summary, threat model, non-goals, and smoke checklist.
- Modify `docs/PROJECT_PLAN.md`
  - change P8 from richer NewAPI pricing snapshots to security and credential governance.
- Modify `docs/PRODUCT_MODEL.md`
  - add `Secret`, `SecretRef`, and redaction responsibilities.
- Modify `README.md`
  - mention encrypted local secret storage only after implementation verifies.

---

## Data Sensitivity Inventory

### High Sensitivity

- `station_keys.api_key`
- `stations.api_key`
- `station_credentials.login_password`
- Authorization headers
- cookies
- sessions
- refresh tokens
- access tokens
- captured request/response headers
- full request prompt bodies
- full upstream response bodies

### Medium Sensitivity

- station base URLs
- station usernames
- balance and quota data
- pricing rules
- request model names
- route policy decisions
- health/cooldown state
- upstream error summaries

### Low Sensitivity

- station display names
- masked key values
- key enabled state
- route strategy names
- aggregate request counts
- token and cost metadata without prompt/response text

---

## Task 1: Correct Phase Documents and Add P8 Phase Summary

**Files:**

- Create: `docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Modify: `README.md`

- [ ] **Step 1: Write the phase document**

Create `docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md` with this structure:

```markdown
# P8 Security and Credential Governance Plan

## Goal

P8 makes Relay Pool Desktop safe enough for long-term use with real Station Keys, station login credentials, captured session metadata, collector snapshots, request logs, route logs, and proxy errors.

## Scope

P8 introduces one SecretManager and one redaction boundary. Business modules keep owning their product behavior, but they no longer store, display, log, import, or export raw secrets directly.

## Sensitive Data

- Station Key API keys
- legacy station API keys
- station login passwords
- token, cookie, session, and authorization values
- collector snapshot raw payloads
- request log error details
- route details and rejected candidates
- import/export backup payloads

## Storage Strategy

Secrets are encrypted before SQLite persistence. The app data encryption key is stored in the host OS keychain. SQLite stores ciphertext, nonce, masked value, hash, and metadata.

## Non-Goals

- no new route strategy
- no new pricing adapter
- no cloud sync
- no team permissions
- no public LAN proxy mode
- no full enterprise audit system

## Completion Standard

- raw SQLite does not contain full keys, passwords, tokens, cookies, prompts, or responses
- UI defaults to masked values
- request logs and collector snapshots are redacted before persistence
- existing plaintext credentials migrate without data loss
- local proxy remains bound to 127.0.0.1
- build, check, and library tests pass
```

- [ ] **Step 2: Correct `docs/PROJECT_PLAN.md` phase order**

Replace the current P8 line that describes NewAPI pricing snapshots with:

```markdown
- P8 安全与凭据治理：统一 SecretManager、本地加密存储、旧明文凭据迁移、UI 脱敏、日志/快照脱敏、导入导出安全边界和本地代理安全复核。
```

- [ ] **Step 3: Extend `docs/PRODUCT_MODEL.md`**

Add:

```markdown
## Secret

`Secret` is encrypted sensitive data owned by a Station, Station Key, collector, proxy runtime, or settings surface.

It owns:

- encrypted value
- masked value
- owner id
- kind
- encryption version
- migration status

Business objects reference secrets through `SecretRef` and never expose full values in list APIs.
```

- [ ] **Step 4: Update README conservatively**

Add an in-progress or completed line depending on execution state:

```markdown
- P8: security and credential governance for encrypted local secrets, redacted logs, safe snapshots, and proxy exposure review.
```

- [ ] **Step 5: Commit**

```powershell
git add -- docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md docs/PROJECT_PLAN.md docs/PRODUCT_MODEL.md README.md
git commit -m "docs: plan p8 security credential governance"
```

---

## Task 2: Add Secret Models and Shared Masking Tests

**Files:**

- Create: `src-tauri/src/models/secrets.rs`
- Create: `src-tauri/src/services/secrets/mod.rs`
- Create: `src-tauri/src/services/secrets/mask.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Write model and masking tests**

Create `src-tauri/src/models/secrets.rs` with structs and tests:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    Station,
    StationKey,
    Collector,
    Proxy,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    ApiKey,
    LoginPassword,
    Token,
    Cookie,
    Session,
    Authorization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    pub id: String,
    pub scope: SecretScope,
    pub owner_id: String,
    pub kind: SecretKind,
    pub masked_value: String,
    pub encryption_version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretMigrationReport {
    pub migrated_count: i64,
    pub skipped_count: i64,
    pub failed_count: i64,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretScanFinding {
    pub table_name: String,
    pub column_name: String,
    pub evidence: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_ref_serializes_camel_case() {
        let value = serde_json::to_value(SecretRef {
            id: "secret-1".to_string(),
            scope: SecretScope::StationKey,
            owner_id: "key-1".to_string(),
            kind: SecretKind::ApiKey,
            masked_value: "sk-...abcd".to_string(),
            encryption_version: 1,
            updated_at: "1000".to_string(),
        })
        .expect("json");

        assert_eq!(value["ownerId"], "key-1");
        assert_eq!(value["maskedValue"], "sk-...abcd");
        assert_eq!(value["encryptionVersion"], 1);
    }
}
```

- [ ] **Step 2: Create shared mask tests**

Create `src-tauri/src/services/secrets/mask.rs`:

```rust
use serde_json::{Map, Value};

const SECRET_HINTS: [&str; 12] = [
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "authorization",
    "cookie",
    "password",
    "secret",
    "session",
    "credential",
];

pub fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "未设置".to_string();
    }
    let prefix: String = trimmed.chars().take(3).collect();
    let suffix: String = trimmed.chars().rev().take(4).collect::<String>().chars().rev().collect();
    if trimmed.chars().count() <= 8 {
        return "••••".to_string();
    }
    format!("{prefix}...{suffix}")
}

pub fn redact_text(text: &str) -> String {
    text.split_whitespace()
        .map(|segment| {
            if looks_like_secret(segment) || segment_has_secret_assignment(segment) {
                "[REDACTED]".to_string()
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = Map::new();
            for (key, child) in map {
                if is_secret_key(key) {
                    next.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    next.insert(key.clone(), redact_value(child));
                }
            }
            Value::Object(next)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(text) if looks_like_secret(text) => Value::String("[REDACTED]".to_string()),
        _ => value.clone(),
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SECRET_HINTS.iter().any(|hint| lower.contains(hint))
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_lowercase();
    value.len() > 18
        && (lower.starts_with("sk-")
            || lower.starts_with("bearer ")
            || lower.contains("authorization")
            || lower.contains("token=")
            || lower.contains("session=")
            || lower.contains("api_key=")
            || lower.contains("password="))
}

fn segment_has_secret_assignment(value: &str) -> bool {
    let lower = value.to_lowercase();
    SECRET_HINTS
        .iter()
        .any(|hint| lower.contains(&format!("{hint}=")) || lower.contains(&format!("{hint}:")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_keeps_prefix_and_suffix_only() {
        let masked = mask_secret("sk-p8-secret-plaintext-canary");
        assert_eq!(masked, "sk-...nary");
        assert!(!masked.contains("secret-plaintext"));
    }

    #[test]
    fn redact_text_removes_bearer_token() {
        let redacted = redact_text("Authorization: Bearer sk-p8-secret-plaintext-canary");
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("sk-p8-secret-plaintext-canary"));
    }

    #[test]
    fn redact_value_removes_nested_cookie() {
        let value = serde_json::json!({
            "headers": {
                "cookie": "rpd_session=p8-cookie-canary"
            },
            "model": "gpt-4o-mini"
        });

        let redacted = redact_value(&value);

        assert_eq!(redacted["headers"]["cookie"], "[REDACTED]");
        assert_eq!(redacted["model"], "gpt-4o-mini");
    }
}
```

- [ ] **Step 3: Export modules**

Modify `src-tauri/src/services/secrets/mod.rs`:

```rust
pub mod mask;
```

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod secrets;
```

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod secrets;
```

- [ ] **Step 4: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml secret_ref_serializes_camel_case --lib
cargo test --manifest-path .\src-tauri\Cargo.toml mask_secret --lib
cargo test --manifest-path .\src-tauri\Cargo.toml redact_value --lib
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/models/secrets.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/services/secrets/mask.rs src-tauri/src/models/mod.rs src-tauri/src/services/mod.rs
git commit -m "feat: add shared secret masking models"
```

---

## Task 3: Add Crypto and Keychain SecretManager

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/services/secrets/crypto.rs`
- Create: `src-tauri/src/services/secrets/keychain.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add dependencies**

Modify `src-tauri/Cargo.toml`:

```toml
aes-gcm = "0.10"
base64 = "0.22"
keyring = "3"
rand = "0.8"
sha2 = "0.10"
```

- [ ] **Step 2: Write crypto tests**

Create `src-tauri/src/services/secrets/crypto.rs`:

```rust
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub ciphertext: String,
    pub nonce: String,
    pub aad: String,
    pub value_hash: String,
}

pub fn generate_data_key() -> [u8; 32] {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn encrypt_secret(key: &[u8; 32], plaintext: &str, aad: &str) -> Result<EncryptedPayload, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|error| format!("初始化加密器失败: {error}"))?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, aes_gcm::aead::Payload { msg: plaintext.as_bytes(), aad: aad.as_bytes() })
        .map_err(|error| format!("加密敏感信息失败: {error}"))?;
    Ok(EncryptedPayload {
        ciphertext: general_purpose::STANDARD.encode(ciphertext),
        nonce: general_purpose::STANDARD.encode(nonce_bytes),
        aad: aad.to_string(),
        value_hash: hash_secret(plaintext),
    })
}

pub fn decrypt_secret(key: &[u8; 32], payload: &EncryptedPayload) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|error| format!("初始化解密器失败: {error}"))?;
    let nonce_bytes = general_purpose::STANDARD
        .decode(&payload.nonce)
        .map_err(|error| format!("解析 nonce 失败: {error}"))?;
    let ciphertext = general_purpose::STANDARD
        .decode(&payload.ciphertext)
        .map_err(|error| format!("解析密文失败: {error}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), aes_gcm::aead::Payload { msg: &ciphertext, aad: payload.aad.as_bytes() })
        .map_err(|_| "解密敏感信息失败，请检查系统凭据是否可用。".to_string())?;
    String::from_utf8(plaintext).map_err(|error| format!("解码敏感信息失败: {error}"))
}

pub fn hash_secret(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    general_purpose::STANDARD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = generate_data_key();
        let payload = encrypt_secret(&key, "sk-p8-secret-plaintext-canary", "station_key:key-1:api_key")
            .expect("encrypt");

        assert_ne!(payload.ciphertext, "sk-p8-secret-plaintext-canary");
        let decrypted = decrypt_secret(&key, &payload).expect("decrypt");
        assert_eq!(decrypted, "sk-p8-secret-plaintext-canary");
    }

    #[test]
    fn decrypt_rejects_wrong_aad() {
        let key = generate_data_key();
        let mut payload = encrypt_secret(&key, "p8-password-canary", "station:station-1:login_password")
            .expect("encrypt");
        payload.aad = "station:station-2:login_password".to_string();

        let result = decrypt_secret(&key, &payload);

        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Add keychain loader**

Create `src-tauri/src/services/secrets/keychain.rs`:

```rust
use base64::{engine::general_purpose, Engine as _};
use keyring::Entry;

use super::crypto::generate_data_key;

const SERVICE: &str = "relay-pool-desktop";
const USERNAME: &str = "local-data-key-v1";

pub fn load_or_create_data_key() -> Result<[u8; 32], String> {
    let entry = Entry::new(SERVICE, USERNAME).map_err(|error| format!("打开系统凭据失败: {error}"))?;
    match entry.get_password() {
        Ok(encoded) => decode_key(&encoded),
        Err(_) => {
            let key = generate_data_key();
            let encoded = general_purpose::STANDARD.encode(key);
            entry
                .set_password(&encoded)
                .map_err(|error| format!("保存系统凭据失败: {error}"))?;
            Ok(key)
        }
    }
}

fn decode_key(encoded: &str) -> Result<[u8; 32], String> {
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("解析系统凭据失败: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| "系统凭据长度不正确。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_rejects_invalid_length() {
        let result = decode_key("abc");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Add SecretManager**

Modify `src-tauri/src/services/secrets/mod.rs`:

```rust
pub mod crypto;
pub mod keychain;
pub mod mask;

#[derive(Clone)]
pub struct SecretManager {
    data_key: [u8; 32],
}

impl SecretManager {
    pub fn initialize() -> Result<Self, String> {
        Ok(Self {
            data_key: keychain::load_or_create_data_key()?,
        })
    }

    pub fn data_key(&self) -> &[u8; 32] {
        &self.data_key
    }
}
```

- [ ] **Step 5: Manage SecretManager in Tauri state**

Modify `src-tauri/src/lib.rs` setup:

```rust
let secret_manager = services::secrets::SecretManager::initialize()?;
app.manage(secret_manager);
```

Place it before commands that can write credentials.

- [ ] **Step 6: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml encrypt_decrypt_round_trip --lib
cargo test --manifest-path .\src-tauri\Cargo.toml decrypt_rejects_wrong_aad --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: crypto tests pass; `cargo check` succeeds.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/services/secrets/crypto.rs src-tauri/src/services/secrets/keychain.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add encrypted secret manager"
```

---

## Task 4: Add Secret Persistence and Safe Migration

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/secrets.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add migration tests**

In `src-tauri/src/services/database.rs` tests, add:

```rust
#[test]
fn migrating_plain_station_key_moves_secret_out_of_plain_column() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key = [7_u8; 32];
    let station = test_station(&database, "p8-migration");
    let station_key = database
        .list_station_keys(station.id.clone())
        .expect("keys")
        .remove(0);

    database
        .migrate_plaintext_secrets_for_tests(&key)
        .expect("migration");

    let connection = database.connection().expect("connection");
    let plain: Option<String> = connection
        .query_row(
            "SELECT api_key FROM station_keys WHERE id = ?1",
            rusqlite::params![station_key.id],
            |row| row.get(0),
        )
        .expect("plain column");
    assert!(plain.is_none());

    let secret_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM secrets WHERE kind = 'api_key'", [], |row| row.get(0))
        .expect("secret count");
    assert!(secret_count >= 1);
}

#[test]
fn migrated_secret_can_be_decrypted_for_routing() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key = [9_u8; 32];
    let station = test_station(&database, "p8-decrypt");

    database
        .migrate_plaintext_secrets_for_tests(&key)
        .expect("migration");

    let candidates = database
        .proxy_route_candidates_with_data_key_for_tests(&key)
        .expect("candidates");

    assert!(candidates.iter().any(|candidate| {
        candidate.station_id == station.id && candidate.api_key == "sk-test-routing"
    }));
}
```

- [ ] **Step 2: Create schema**

In database initialization, add:

```rust
connection.execute_batch(
    r#"
    CREATE TABLE IF NOT EXISTS secrets (
        id TEXT PRIMARY KEY,
        scope TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        ciphertext TEXT NOT NULL,
        nonce TEXT NOT NULL,
        aad TEXT NOT NULL,
        masked_value TEXT NOT NULL,
        value_hash TEXT NOT NULL,
        encryption_version INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_secrets_owner_kind
    ON secrets(owner_id, kind);

    CREATE TABLE IF NOT EXISTS secret_migration_events (
        id TEXT PRIMARY KEY,
        owner_table TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        secret_kind TEXT NOT NULL,
        status TEXT NOT NULL,
        error_message TEXT,
        created_at TEXT NOT NULL
    );
    "#,
)?;
```

Use existing migration helper style to add:

```sql
ALTER TABLE station_keys ADD COLUMN api_key_secret_id TEXT;
ALTER TABLE stations ADD COLUMN api_key_secret_id TEXT;
ALTER TABLE station_credentials ADD COLUMN login_password_secret_id TEXT;
```

- [ ] **Step 3: Add secret insert/read helpers**

Add functions:

```rust
fn secret_aad(scope: &str, owner_id: &str, kind: &str) -> String {
    format!("{scope}:{owner_id}:{kind}")
}

fn upsert_secret_in_connection(
    connection: &Connection,
    data_key: &[u8; 32],
    scope: &str,
    owner_id: &str,
    kind: &str,
    plaintext: &str,
) -> Result<String, String> {
    let id = generate_id("secret");
    let now = now_string();
    let aad = secret_aad(scope, owner_id, kind);
    let encrypted = crate::services::secrets::crypto::encrypt_secret(data_key, plaintext, &aad)?;
    let masked = crate::services::secrets::mask::mask_secret(plaintext);
    connection.execute(
        "INSERT INTO secrets (
            id, scope, owner_id, kind, ciphertext, nonce, aad, masked_value,
            value_hash, encryption_version, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)",
        params![
            id,
            scope,
            owner_id,
            kind,
            encrypted.ciphertext,
            encrypted.nonce,
            encrypted.aad,
            masked,
            encrypted.value_hash,
            now,
            now
        ],
    )
    .map_err(|error| format!("保存加密凭据失败: {error}"))?;
    Ok(id)
}
```

- [ ] **Step 4: Migrate rows transactionally by row**

Add:

```rust
pub fn migrate_plaintext_secrets(&self, data_key: &[u8; 32]) -> Result<SecretMigrationReport, String> {
    let connection = self.connection()?;
    migrate_plaintext_secrets_in_connection(&connection, data_key)
}
```

Migration behavior:

- station_keys: encrypt non-empty `api_key`, write `api_key_secret_id`, set `api_key = NULL`;
- stations: encrypt non-empty legacy `api_key`, write `api_key_secret_id`, set `api_key = NULL`;
- station_credentials: encrypt non-empty `login_password`, write `login_password_secret_id`, set `login_password = NULL`;
- each row has its own transaction savepoint so one bad row does not block others;
- failures are recorded in `secret_migration_events`.

- [ ] **Step 5: Run migration tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml migrating_plain_station_key_moves_secret_out_of_plain_column --lib
cargo test --manifest-path .\src-tauri\Cargo.toml migrated_secret_can_be_decrypted_for_routing --lib
```

Expected: both tests pass.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/models/secrets.rs
git commit -m "feat: migrate plaintext credentials to encrypted secrets"
```

---

## Task 5: Route Station Key and Credential Writes Through SecretManager

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`

- [ ] **Step 1: Write station key preserve-secret test**

In database tests:

```rust
#[test]
fn updating_station_key_with_blank_api_key_preserves_encrypted_secret() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let data_key = [3_u8; 32];
    let station = test_station(&database, "p8-preserve-key");
    database.migrate_plaintext_secrets_for_tests(&data_key).expect("migration");
    let key = database.list_station_keys(station.id.clone()).expect("keys").remove(0);

    database
        .update_station_key_with_data_key_for_tests(
            &data_key,
            UpdateStationKeyInput {
                id: key.id.clone(),
                station_id: station.id,
                name: "Renamed".to_string(),
                api_key: None,
                enabled: true,
                priority: key.priority,
                group_name: None,
                tier_label: None,
                status: "active".to_string(),
                note: None,
            },
        )
        .expect("update");

    let secret = database
        .resolve_station_key_secret_for_tests(&data_key, &key.id)
        .expect("resolve");

    assert_eq!(secret, "sk-test-routing");
}
```

- [ ] **Step 2: Add command signatures with SecretManager state**

Modify commands that create, update, or resolve secrets:

```rust
pub fn create_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: CreateStationKeyInput,
) -> Result<StationKey, String> {
    database.create_station_key_with_secrets(input, secrets.data_key())
}

pub fn update_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationKeyInput,
) -> Result<StationKey, String> {
    database.update_station_key_with_secrets(input, secrets.data_key())
}
```

Apply the same pattern to:

- `create_station`
- `update_station`
- `update_station_credentials`
- collector login calls that need `get_station_login_password`
- proxy runtime candidate loading

- [ ] **Step 3: Resolve API keys only at proxy execution boundary**

Keep `RouteCandidate.api_key` runtime-only:

```rust
pub struct RouteCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub upstream_base_url: String,
    pub api_key: String,
    pub upstream_api_format: UpstreamApiFormat,
    pub priority: i64,
}
```

`proxy_route_candidates` should decrypt API keys using the managed data key and never expose this function through frontend commands.

- [ ] **Step 4: Update collector credential access**

`get_station_login_password` must decrypt through `SecretManager`:

```rust
pub fn get_station_login_password(&self, station_id: String, data_key: &[u8; 32]) -> Result<Option<String>, String>
```

The returned password is used in collector runtime memory only and never serialized to frontend output.

- [ ] **Step 5: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml updating_station_key_with_blank_api_key_preserves_encrypted_secret --lib
cargo test --manifest-path .\src-tauri\Cargo.toml proxy --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: tests pass and proxy still compiles.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/sub2api.rs
git commit -m "feat: route credential writes through secret manager"
```

---

## Task 6: Enforce Redaction for Logs, Snapshots, Route Details, and Errors

**Files:**

- Modify: `src-tauri/src/services/capture/redaction.rs`
- Modify: `src-tauri/src/services/capture/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/logs/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write redaction persistence tests**

Add tests:

```rust
#[test]
fn request_log_redacts_error_and_route_details_before_persistence() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let log = database
        .insert_request_log(CreateRequestLogInput {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            stream: false,
            status: "error".to_string(),
            station_key_id: Some("key-1".to_string()),
            station_id: Some("station-1".to_string()),
            upstream_base_url: Some("https://example.test/v1".to_string()),
            fallback_count: 1,
            error_message: Some("upstream rejected Bearer sk-p8-secret-plaintext-canary".to_string()),
            route_policy: Some("priority_fallback".to_string()),
            route_reason: Some("selected key-1".to_string()),
            rejected_candidates_json: Some(r#"[{"reason":"token=abc12345678901234567890"}]"#.to_string()),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            estimated_input_cost: None,
            estimated_output_cost: None,
            estimated_total_cost: None,
            cost_currency: None,
            pricing_rule_id: None,
            pricing_source: None,
            cost_status: Some("unknown_usage".to_string()),
            started_at: "1000".to_string(),
            finished_at: Some("1001".to_string()),
            duration_ms: Some(1),
        })
        .expect("insert log");

    assert!(!log.error_message.unwrap_or_default().contains("sk-p8-secret-plaintext-canary"));
    assert!(!log.rejected_candidates_json.unwrap_or_default().contains("abc12345678901234567890"));
}

#[test]
fn collector_snapshot_redacts_raw_secret_fields_before_persistence() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "p8-snapshot-redaction");
    let snapshot = database
        .insert_collector_snapshot(
            &station.id,
            "test",
            "success",
            serde_json::json!({"message": "ok"}),
            serde_json::json!({"models": []}),
            Some(serde_json::json!({
                "headers": { "authorization": "Bearer sk-p8-secret-plaintext-canary" },
                "cookie": "rpd_session=p8-cookie-canary"
            })),
            None,
        )
        .expect("snapshot");

    let text = serde_json::to_string(&snapshot.raw_json_redacted).expect("json");
    assert!(!text.contains("sk-p8-secret-plaintext-canary"));
    assert!(!text.contains("p8-cookie-canary"));
}
```

- [ ] **Step 2: Delegate capture redaction to shared mask module**

Change `src-tauri/src/services/capture/redaction.rs` to call:

```rust
pub use crate::services::secrets::mask::{redact_text as redact_text_preview, redact_value};
```

If `redact_text_preview` needs the 4,000-character limit, wrap `redact_text`:

```rust
pub fn redact_text_preview(text: &str) -> String {
    let limited: String = text.chars().take(4_000).collect();
    let redacted = crate::services::secrets::mask::redact_text(&limited);
    if text.chars().count() > 4_000 {
        format!("{redacted}\n... 已截断")
    } else {
        redacted
    }
}
```

- [ ] **Step 3: Redact before request log insert**

Inside `insert_request_log_in_connection`, sanitize:

```rust
let error_message = input
    .error_message
    .as_deref()
    .map(crate::services::secrets::mask::redact_text);
let rejected_candidates_json = input
    .rejected_candidates_json
    .as_deref()
    .map(crate::services::secrets::mask::redact_text);
```

Use these sanitized values in SQL params.

- [ ] **Step 4: Redact proxy OpenAI-style errors**

In `openai_error` or before `ProxyResponse::json_error`, call shared redaction:

```rust
let message = crate::services::secrets::mask::redact_text(message);
```

- [ ] **Step 5: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log_redacts_error --lib
cargo test --manifest-path .\src-tauri\Cargo.toml collector_snapshot_redacts_raw_secret_fields --lib
cargo test --manifest-path .\src-tauri\Cargo.toml redact --lib
```

Expected: all redaction tests pass.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/capture/redaction.rs src-tauri/src/services/capture/mod.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/sub2api.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/logs/mod.rs src-tauri/src/services/database.rs
git commit -m "feat: redact logs snapshots and proxy errors"
```

---

## Task 7: Add Security Audit Commands and SQLite Safety Scan

**Files:**

- Create: `src-tauri/src/services/secrets/audit.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add scan tests**

Create `src-tauri/src/services/secrets/audit.rs`:

```rust
use crate::models::secrets::SecretScanFinding;

pub fn canary_patterns() -> Vec<&'static str> {
    vec![
        "sk-p8-secret-plaintext-canary",
        "p8-password-canary",
        "rpd_session=p8-cookie-canary",
        "Bearer sk-p8-secret",
        "token=p8-token-canary",
    ]
}

pub fn evidence_for_value(value: &str) -> String {
    let mut chars = value.chars();
    let preview: String = chars.by_ref().take(24).collect();
    if value.chars().count() > 24 {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_short() {
        let evidence = evidence_for_value("sk-p8-secret-plaintext-canary-extra");
        assert!(evidence.ends_with("..."));
        assert!(evidence.chars().count() <= 27);
    }
}
```

- [ ] **Step 2: Add database scan**

In `AppDatabase`, add:

```rust
pub fn run_secret_safety_scan(&self) -> Result<Vec<SecretScanFinding>, String> {
    let connection = self.connection()?;
    let patterns = crate::services::secrets::audit::canary_patterns();
    let targets = [
        ("stations", "api_key"),
        ("station_keys", "api_key"),
        ("station_credentials", "login_password"),
        ("collector_snapshots", "raw_json_redacted"),
        ("collector_snapshots", "error_message"),
        ("request_logs", "error_message"),
        ("request_logs", "rejected_candidates_json"),
    ];
    let mut findings = Vec::new();
    for (table_name, column_name) in targets {
        let sql = format!("SELECT {column_name} FROM {table_name} WHERE {column_name} IS NOT NULL");
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("准备安全扫描失败: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .map_err(|error| format!("执行安全扫描失败: {error}"))?;
        for row in rows {
            let value = row.map_err(|error| format!("读取安全扫描结果失败: {error}"))?.unwrap_or_default();
            if patterns.iter().any(|pattern| value.contains(pattern)) {
                findings.push(SecretScanFinding {
                    table_name: table_name.to_string(),
                    column_name: column_name.to_string(),
                    evidence: crate::services::secrets::audit::evidence_for_value(&value),
                });
            }
        }
    }
    Ok(findings)
}
```

- [ ] **Step 3: Add commands**

In `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn run_secret_safety_scan(database: State<'_, AppDatabase>) -> Result<Vec<SecretScanFinding>, String> {
    database.run_secret_safety_scan()
}

#[tauri::command]
pub fn get_secret_migration_status(database: State<'_, AppDatabase>) -> Result<SecretMigrationReport, String> {
    database.secret_migration_status()
}
```

Register both commands in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml evidence_is_short --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: tests and check pass.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/secrets/audit.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add local secret safety scan"
```

---

## Task 8: Upgrade UI Secret Display and Security Status

**Files:**

- Modify: `src/components/ui/MaskedSecret.tsx`
- Create: `src/lib/types/secrets.ts`
- Create: `src/lib/api/secrets.ts`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`

- [ ] **Step 1: Upgrade MaskedSecret**

Replace `src/components/ui/MaskedSecret.tsx` with:

```tsx
import { Copy, Eye, EyeOff } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

type MaskedSecretProps = {
  value: string;
  present?: boolean;
  revealLabel?: string;
  onReveal?: () => Promise<string>;
  onCopy?: (value: string) => Promise<void>;
};

export function MaskedSecret({ value, present = true, revealLabel = "查看", onReveal, onCopy }: MaskedSecretProps) {
  const [revealed, setRevealed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const displayValue = revealed ?? (present ? value : "未设置");

  async function handleReveal() {
    if (!onReveal) return;
    setBusy(true);
    try {
      if (revealed) {
        setRevealed(null);
      } else {
        setRevealed(await onReveal());
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleCopy() {
    const copyValue = revealed ?? value;
    if (onCopy) {
      await onCopy(copyValue);
    } else {
      await navigator.clipboard.writeText(copyValue);
    }
  }

  return (
    <span className="inline-flex items-center gap-1">
      <code className="rounded border border-border bg-slate-50 px-1.5 py-0.5 text-xs text-slate-700">
        {displayValue}
      </code>
      {onReveal ? (
        <Button type="button" variant="ghost" className="h-6 px-1.5 text-xs" onClick={handleReveal} disabled={busy || !present}>
          {revealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          <span className="sr-only">{revealLabel}</span>
        </Button>
      ) : null}
      <Button type="button" variant="ghost" className="h-6 px-1.5 text-xs" onClick={handleCopy} disabled={!present}>
        <Copy className="h-3.5 w-3.5" />
        <span className="sr-only">复制</span>
      </Button>
    </span>
  );
}
```

P8 may omit `onReveal` on pages that should never reveal secrets.

- [ ] **Step 2: Add frontend API**

Create `src/lib/types/secrets.ts`:

```ts
export type SecretMigrationReport = {
  migratedCount: number;
  skippedCount: number;
  failedCount: number;
  failures: string[];
};

export type SecretScanFinding = {
  tableName: string;
  columnName: string;
  evidence: string;
};
```

Create `src/lib/api/secrets.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { SecretMigrationReport, SecretScanFinding } from "@/lib/types/secrets";

export function getSecretMigrationStatus() {
  return invoke<SecretMigrationReport>("get_secret_migration_status");
}

export function runSecretSafetyScan() {
  return invoke<SecretScanFinding[]>("run_secret_safety_scan");
}
```

- [ ] **Step 3: Replace raw masked rendering**

Use `MaskedSecret` for:

- `KeyPoolPage.tsx` key rows;
- `StationsPage.tsx` station key rows;
- any dashboard or settings masked local key display.

Do not pass raw secret values from list APIs.

- [ ] **Step 4: Add Settings security status panel**

In `SettingsPage.tsx`, load:

```ts
const [secretMigration, setSecretMigration] = useState<SecretMigrationReport | null>(null);
const [scanFindings, setScanFindings] = useState<SecretScanFinding[]>([]);
```

Render:

```text
加密状态：已启用本机加密存储
迁移：已迁移 X 项，失败 Y 项
安全扫描：未发现明文 canary / 发现 N 项需处理
```

- [ ] **Step 5: Redact UI error rendering**

Any UI display of:

- `errorMessage`
- `rejectedCandidatesJson`
- snapshot developer JSON

must be shown as returned from backend and must not reconstruct raw request bodies.

- [ ] **Step 6: Run frontend build**

```powershell
pnpm build
```

Expected: build passes.

- [ ] **Step 7: Commit**

```powershell
git add -- src/components/ui/MaskedSecret.tsx src/lib/types/secrets.ts src/lib/api/secrets.ts src/features/key-pool/KeyPoolPage.tsx src/features/stations/StationsPage.tsx src/features/logs/LogsPage.tsx src/features/routing/RoutingPage.tsx src/features/collectors/CollectorsPage.tsx src/features/settings/SettingsPage.tsx
git commit -m "feat: unify masked secret UI"
```

---

## Task 9: Define Import, Export, and Backup Safety Boundaries

**Files:**

- Create: `docs/SECURITY_EXPORT_IMPORT.md`
- Modify: `docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md`
- Modify: `README.md`

- [ ] **Step 1: Create export/import policy document**

Create `docs/SECURITY_EXPORT_IMPORT.md`:

```markdown
# Security Export and Import Policy

## Default Export

Default exports do not include raw API keys, station login passwords, cookies, sessions, tokens, authorization headers, prompts, responses, or encrypted ciphertext.

Default exports may include:

- station display name
- station type
- base URL
- masked key value
- key enabled state
- routing policy metadata
- pricing and balance metadata
- request log metadata without prompt or response text

## Secret Export

Encrypted secret export is not part of P8. If added in a later phase, it must require explicit user confirmation and password-based encryption.

## Import

Imports may create stations, key metadata, pricing rules, aliases, and routing settings. Imports do not silently overwrite existing secrets. A user must paste new secret values through the normal credential forms.

## Backups

SQLite database backups include encrypted secret ciphertext. A backup remains tied to the system keychain entry unless a later encrypted-export flow is implemented.
```

- [ ] **Step 2: Reference policy from P8 phase doc**

Add:

```markdown
Import/export follows `docs/SECURITY_EXPORT_IMPORT.md`. P8 default exports never include raw secrets or encrypted secret payloads.
```

- [ ] **Step 3: Update README security note**

Add:

```markdown
Security note: default exports and logs are metadata-only. Real keys, passwords, tokens, cookies, prompts, and responses are excluded from default export paths.
```

- [ ] **Step 4: Commit**

```powershell
git add -- docs/SECURITY_EXPORT_IMPORT.md docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md README.md
git commit -m "docs: define secret export import boundaries"
```

---

## Task 10: Local Proxy Security Review and Tests

**Files:**

- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md`

- [ ] **Step 1: Add proxy bind test**

In `runtime.rs` tests:

```rust
#[test]
fn proxy_status_reports_localhost_bind_only() {
    let proxy = ProxyRuntimeState::default();
    let status = proxy.status(8787);

    assert_eq!(status.bind_addr, "127.0.0.1");
    assert_ne!(status.bind_addr, "0.0.0.0");
}
```

- [ ] **Step 2: Tighten CORS documentation and test**

Keep OpenAI-compatible local browser compatibility, but document that CORS is for local proxy endpoint only and does not expose Tauri management APIs.

Existing test can remain:

```rust
assert!(text.contains("access-control-allow-methods: GET, POST, OPTIONS"));
assert!(text.contains("access-control-allow-headers: authorization, content-type, accept"));
```

Add:

```rust
assert!(!text.to_lowercase().contains("x-tauri"));
```

- [ ] **Step 3: Ensure proxy error redaction**

Add test:

```rust
#[test]
fn openai_error_redacts_secret_like_message() {
    let value = openai_error("upstream said Bearer sk-p8-secret-plaintext-canary", "upstream_error");
    let text = serde_json::to_string(&value).expect("json");

    assert!(!text.contains("sk-p8-secret-plaintext-canary"));
    assert!(text.contains("[REDACTED]"));
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml proxy_status_reports_localhost_bind_only --lib
cargo test --manifest-path .\src-tauri\Cargo.toml openai_error_redacts_secret_like_message --lib
cargo test --manifest-path .\src-tauri\Cargo.toml cors --lib
```

Expected: proxy security tests pass.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/mod.rs docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md
git commit -m "test: verify local proxy security boundaries"
```

---

## Task 11: End-to-End Security Verification

**Files:**

- No source changes unless verification exposes a bug.

- [ ] **Step 1: Run automated checks**

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected:

- all commands exit 0;
- no new dead code warnings;
- no test output prints raw keys, passwords, cookies, tokens, prompts, or responses.

- [ ] **Step 2: Start app**

```powershell
pnpm tauri:dev
```

Expected:

- app starts;
- database migration completes;
- Settings security panel reports encryption available.

- [ ] **Step 3: Create canary test data through UI**

Use UI forms only, not shell history:

```text
Station Key API key: sk-p8-secret-plaintext-canary
Station login password: p8-password-canary
Cookie-like collector test value: rpd_session=p8-cookie-canary
Prompt canary: p8-prompt-canary
Response canary: p8-response-canary
```

- [ ] **Step 4: Restart app**

Close and run:

```powershell
pnpm tauri:dev
```

Expected:

- Station Key still shows as present;
- login password still shows as present;
- local proxy can still use the Station Key if the upstream is valid.

- [ ] **Step 5: Scan SQLite for canaries**

Find app database path from startup log. Then run:

```powershell
$DB = "<database-path-from-app-log>"
Select-String -Path $DB -Pattern "sk-p8-secret-plaintext-canary","p8-password-canary","rpd_session=p8-cookie-canary","p8-prompt-canary","p8-response-canary" -SimpleMatch
```

Expected:

- no matches.

If `Select-String` cannot read the SQLite file reliably, run the in-app `run_secret_safety_scan` command from Settings security panel and confirm zero findings.

- [ ] **Step 6: Verify request log redaction**

Start local proxy and send a test request using UI-managed key. Then open Logs.

Expected:

- log row includes path, model, status, duration, key name or masked key, station, route policy, token/cost metadata;
- log row does not include `sk-p8-secret-plaintext-canary`;
- log row does not include `p8-prompt-canary`;
- log row does not include `p8-response-canary`;
- route details do not include Authorization, cookie, token, session, or password values.

- [ ] **Step 7: Verify collector snapshot redaction**

Use collector or WebView capture with a controlled canary in captured payload.

Expected:

- developer JSON says redacted;
- copied developer JSON does not include `rpd_session=p8-cookie-canary`;
- copied developer JSON does not include full Authorization or API key values.

- [ ] **Step 8: Verify UI masking**

Inspect:

- Key Pool
- Stations
- Settings
- Logs
- Routing simulator
- Collector developer JSON

Expected:

- default UI never shows full API key;
- password is shown as present/not present, not plaintext;
- explicit reveal exists only where the backend command supports it;
- blank API key edit preserves old encrypted secret.

- [ ] **Step 9: Verify local proxy exposure**

With proxy running:

```powershell
netstat -ano | findstr :<proxy-port>
curl.exe http://127.0.0.1:<proxy-port>/v1/models
curl.exe http://localhost:<proxy-port>/v1/models
```

Expected:

- bind address is `127.0.0.1`;
- local requests work;
- no `0.0.0.0:<proxy-port>` listener appears.

- [ ] **Step 10: Final git and evidence report**

```powershell
git status --short
git log --oneline -8
```

Report:

- commits made;
- automated verification results;
- manual smoke results;
- whether SQLite canary scan passed;
- any remaining risk;
- whether P8 is ready to push.

---

## Suggested Commit Sequence

1. `docs: plan p8 security credential governance`
2. `feat: add shared secret masking models`
3. `feat: add encrypted secret manager`
4. `feat: migrate plaintext credentials to encrypted secrets`
5. `feat: route credential writes through secret manager`
6. `feat: redact logs snapshots and proxy errors`
7. `feat: add local secret safety scan`
8. `feat: unify masked secret UI`
9. `docs: define secret export import boundaries`
10. `test: verify local proxy security boundaries`

Use exact staging paths. Do not use `git add .`, `git add -A`, or `git commit -a`.

---

## P8 Pass/Fail Rubric

P8 passes when the app can answer these with evidence:

```text
真实 Station Key 存在哪里？只在 secrets 密文里。
真实登录密码存在哪里？只在 secrets 密文里。
路由器如何拿到 key？通过 SecretManager 在代理运行边界解密。
UI 为什么不会泄露 key？列表 API 只返回 masked/present，组件统一显示 masked。
日志为什么不会泄露？insert_request_log 前统一 redaction。
采集快照为什么不会泄露？snapshot 入库前统一 redaction。
旧明文数据怎么办？迁移到 secrets，验证成功后清空旧列。
如果迁移失败怎么办？保留原值，记录失败，不破坏用户数据。
导出会不会带走真实 key？默认导出不包含 raw secret 或 ciphertext。
本地代理有没有暴露到局域网？只监听 127.0.0.1。
```

P8 fails if any of these are true:

- raw SQLite contains a known canary API key after migration;
- raw SQLite contains a known canary password after migration;
- request logs contain prompt text or response text;
- request logs contain full API key, token, cookie, session, or password;
- collector snapshots contain raw Authorization, cookie, token, session, or password;
- frontend list APIs return raw secret values;
- Station Key edit with blank key clears the old encrypted key unintentionally;
- proxy cannot route after secret migration;
- migration failure deletes or corrupts existing user credentials;
- local proxy binds to `0.0.0.0`;
- docs still describe P8 as NewAPI pricing work.
