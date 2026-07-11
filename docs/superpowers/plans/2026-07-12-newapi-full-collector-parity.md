# NewAPI Full Collector Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make NewAPI stations support the same authenticated collection and remote-Key workflows as Sub2API, plus NewAPI model collection, without introducing a parallel data model or leaking secrets.

**Architecture:** Keep NewAPI as its own adapter and split parsing, authentication, HTTP behavior, and remote-Key operations into focused Rust modules. Reuse the existing collector facts, snapshot, group/rate, balance, remote discovery, StationKey encryption, proxy, and UI boundaries; preserve all-or-nothing replacement for paginated remote-Key scans and use an explicit non-idempotent create state machine.

**Tech Stack:** Rust 2021, Tauri 2, rusqlite, ureq, serde_json, React 18, TypeScript 5.7, Vite 6, Node source-contract tests

---

## Execution Preconditions

- Execute from an isolated worktree created with `superpowers:using-git-worktrees` from commit `a90b1d1` or a later commit containing the approved design.
- Do not copy the current root worktree's unrelated modifications into the implementation branch.
- Re-run `git status --short` immediately before execution and preserve every unrelated path. At plan creation, the root worktree contains unrelated Dashboard/model-price files and scripts; none belong in the NewAPI implementation commits.
- Before each commit, stage only the exact paths listed in that task. Never use `git add .`, `git add -A`, or `git commit -a`.
- Do not place the supplied real account, password, Cookie, user ID, access token, or full remote Key in files, command history, fixtures, test output, or git history.
- Use upstream `QuantumNous/new-api` commit `bde9b2f44887d34ec54799ae191d50f97914359e` as the reproducible protocol reference. Re-check upstream HEAD before the live test and record only contract differences, never copied AGPL implementation.

## Planned File Map

- Replace `src-tauri/src/services/collectors/adapters/newapi.rs` with `src-tauri/src/services/collectors/adapters/newapi/mod.rs` as the adapter entry point.
- Create `src-tauri/src/services/collectors/adapters/newapi/parsers.rs` for response envelope, status, balance, group, model, and remote-token parsing.
- Create `src-tauri/src/services/collectors/adapters/newapi/auth.rs` for credential selection, login, Cookie normalization, persistence, and one-shot auth recovery.
- Create `src-tauri/src/services/collectors/adapters/newapi/client.rs` for bounded HTTP execution, idempotency-aware retry, endpoint diagnostics, and pagination.
- Create `src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs` for scan, reveal, create, reconcile, and test-only live cleanup.
- Create `src-tauri/src/services/collectors/adapters/newapi/test_support.rs` under `#[cfg(test)]` for reusable loopback HTTP fixtures shared by auth, collection, and remote-Key tests.
- Modify `src-tauri/src/models/credentials.rs` and `src-tauri/src/services/database.rs` for atomic internally-sourced session persistence and Cookie-only readiness.
- Modify `src-tauri/src/services/collectors/mod.rs` for NewAPI Full task composition and station-type login dispatch.
- Modify `src-tauri/src/services/remote_keys.rs` for NewAPI full-secret dispatch and IPC secret suppression.
- Modify `src/features/collectors/CollectorsPage.tsx`, `src/features/stations/AddProviderPage.tsx`, and the existing station-key API/types only where the established UI flow needs NewAPI truthfulness.
- Create `scripts/newapi-collector-contract.test.mjs` for cross-boundary source contracts that are not economical to express through the current frontend test stack.

### Task 1: Split the NewAPI adapter without changing behavior

**Files:**
- Delete: `src-tauri/src/services/collectors/adapters/newapi.rs`
- Create: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Move the existing adapter into the module directory**

Run:

```powershell
New-Item -ItemType Directory -Force src-tauri\src\services\collectors\adapters\newapi | Out-Null
git mv src-tauri\src\services\collectors\adapters\newapi.rs src-tauri\src\services\collectors\adapters\newapi\mod.rs
```

Expected: `git status --short` shows one rename and no unrelated path staged.

- [ ] **Step 2: Prove Rust resolves the module from `newapi/mod.rs`**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_
```

Expected: the existing `newapi_quota_converts_to_usd_units` and `newapi_groups_parse_list_and_rate_fields` tests pass.

- [ ] **Step 3: Run a compile check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: exit code 0.

- [ ] **Step 4: Commit the mechanical split**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi.rs src-tauri/src/services/collectors/adapters/newapi/mod.rs
git commit -m "refactor: split NewAPI adapter module"
```

### Task 2: Normalize NewAPI envelopes, balances, groups, and models

**Files:**
- Create: `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Add failing parser tests**

Create `parsers.rs` with tests that lock the real response shapes:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_requires_success_and_returns_data() {
        let payload = json!({"success": true, "message": "", "data": {"quota": 750000}});
        assert_eq!(envelope_data(&payload).expect("data")["quota"], 750000);
        let failed = json!({"success": false, "message": "not logged in", "data": null});
        assert_eq!(envelope_data(&failed).unwrap_err().message, "not logged in");
    }

    #[test]
    fn balance_uses_runtime_quota_per_unit() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 750000, "used_quota": 250000}),
            250000.0,
            false,
        );
        assert_eq!(fact.value, Some(3.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(4.0));
        assert_eq!(fact.confidence, 0.95);
    }

    #[test]
    fn group_map_preserves_names_and_non_numeric_rates() {
        let facts = parse_group_facts(
            "station-1",
            &json!({
                "default": {"desc": "Default", "ratio": 1.0},
                "auto": {"desc": "Automatic", "ratio": "自动"}
            }),
        );
        assert_eq!(facts.groups.len(), 2);
        assert!(facts.groups.iter().any(|group| group.group_name == "default"));
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "auto" && rate.effective_rate_multiplier.is_none()
        }));
    }

    #[test]
    fn models_accept_strings_and_objects_without_duplicates() {
        let models = parse_models(
            "station-1",
            &json!(["gpt-4.1-mini", {"id": "claude-sonnet"}, {"name": "gpt-4.1-mini"}]),
        );
        assert_eq!(models.iter().map(|model| model.model.as_str()).collect::<Vec<_>>(), vec![
            "gpt-4.1-mini",
            "claude-sonnet",
        ]);
    }
}
```

- [ ] **Step 2: Register the parser module and verify RED**

Add to `newapi/mod.rs`:

```rust
mod parsers;
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast collectors::adapters::newapi::parsers::tests
```

Expected: compilation fails because `NewApiEnvelopeError`, `envelope_data`, `parse_balance_fact`, `parse_group_facts`, and `parse_models` are not defined.

- [ ] **Step 3: Implement the pure parser contract**

Add these public module-local APIs:

```rust
use std::collections::HashSet;
use serde_json::Value;
use crate::services::collectors::facts::{
    CollectedBalanceFact, CollectedGroupFact, CollectedModelFact, CollectedRateFact, CollectorFacts,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NewApiEnvelopeError {
    pub message: String,
}

pub(super) fn envelope_data(payload: &Value) -> Result<&Value, NewApiEnvelopeError> {
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(NewApiEnvelopeError {
            message: payload.get("message").and_then(Value::as_str).unwrap_or("NewAPI request failed").to_string(),
        });
    }
    payload.get("data").ok_or_else(|| NewApiEnvelopeError {
        message: "NewAPI response is missing data".to_string(),
    })
}

pub(super) fn parse_models(station_id: &str, data: &Value) -> Vec<CollectedModelFact> {
    let mut seen = HashSet::new();
    data.as_array().into_iter().flatten().filter_map(|value| {
        let name = value.as_str().or_else(|| {
            ["id", "name", "model"].into_iter().find_map(|key| value.get(key).and_then(Value::as_str))
        })?.trim();
        if name.is_empty() || !seen.insert(name.to_string()) { return None; }
        Some(CollectedModelFact {
            station_id: station_id.to_string(),
            model: name.to_string(),
            available: true,
            source: "newapi_user_models".to_string(),
            confidence: 0.9,
        })
    }).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NewApiStatus {
    pub system_name: Option<String>,
    pub quota_per_unit: f64,
    pub quota_display_type: Option<String>,
    pub used_fallback: bool,
}

pub(super) fn parse_status(data: &Value) -> NewApiStatus {
    let quota_per_unit = parse_optional_f64(data.get("quota_per_unit"));
    NewApiStatus {
        system_name: data.get("system_name").and_then(Value::as_str).map(ToString::to_string),
        quota_per_unit: quota_per_unit.filter(|value| *value > 0.0).unwrap_or(500000.0),
        quota_display_type: data.get("quota_display_type").and_then(Value::as_str).map(ToString::to_string),
        used_fallback: quota_per_unit.is_none_or(|value| value <= 0.0),
    }
}

pub(super) fn parse_balance_fact(
    station_id: &str,
    data: &Value,
    quota_per_unit: f64,
    quota_per_unit_fallback: bool,
) -> CollectedBalanceFact {
    let remaining = parse_optional_f64(data.get("quota")).map(|value| value / quota_per_unit);
    let used = parse_optional_f64(data.get("used_quota")).map(|value| value / quota_per_unit);
    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: remaining,
        used_value: used,
        total_value: remaining.zip(used).map(|(left, right)| left + right),
        currency: "USD".to_string(),
        credit_unit: Some(format!("newapi_quota_{quota_per_unit}")),
        status: if remaining == Some(0.0) { "depleted" } else { "normal" }.to_string(),
        source: "newapi_user_self".to_string(),
        confidence: if quota_per_unit_fallback { 0.75 } else { 0.95 },
        collected_at: None,
    }
}

pub(super) fn parse_group_facts(station_id: &str, data: &Value) -> CollectorFacts {
    let mut facts = CollectorFacts::default();
    for (group_name, value) in data.as_object().into_iter().flatten() {
        let group_key_hash = super::stable_group_key_hash(
            station_id,
            "newapi",
            Some(group_name.as_str()),
            group_name,
        );
        let rate = parse_optional_f64(value.get("ratio"));
        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: Some(group_name.clone()),
            group_key_hash: group_key_hash.clone(),
            group_name: group_name.clone(),
            visibility: "available".to_string(),
            source: "newapi_user_groups".to_string(),
            confidence: 0.9,
            raw_json_redacted: Some(crate::services::secrets::mask::redact_value(value)),
        });
        facts.rates.push(CollectedRateFact {
            station_id: station_id.to_string(),
            station_key_id: None,
            group_id: Some(group_name.clone()),
            group_key_hash,
            group_name: group_name.clone(),
            default_rate_multiplier: rate,
            user_rate_multiplier: rate,
            effective_rate_multiplier: rate,
            source: "newapi_user_groups".to_string(),
            confidence: if rate.is_some() { 0.9 } else { 0.65 },
            checked_at: None,
            raw_json_redacted: None,
        });
    }
    facts
}

fn parse_optional_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| value.as_f64().or_else(|| value.as_str()?.trim().parse::<f64>().ok()))
}
```

Implement the three declared functions with the exact assertions above. `parse_status` accepts positive numeric `quota_per_unit` values and otherwise returns `500000.0` with `used_fallback = true`. Reuse a single `parse_optional_f64` helper for JSON numbers and numeric strings. Generate group hashes from the station ID, adapter name, and upstream map key; do not use the description as identity.

- [ ] **Step 4: Make the old adapter use the new parser functions**

Replace the old top-level payload reads in `newapi/mod.rs`:

```rust
let data = parsers::envelope_data(&payload).map_err(|error| error.message)?;
facts.balances.push(parsers::parse_balance_fact(
    station_id,
    data,
    quota_per_unit,
    quota_per_unit_fallback,
));
```

For Models snapshots, always build:

```rust
let model_names = models.iter().map(|model| model.model.clone()).collect::<Vec<_>>();
let normalized_json = json!({ "models": model_names });
```

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast collectors::adapters::newapi::parsers::tests
```

Expected: all four parser tests pass.

- [ ] **Step 6: Commit parser normalization**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/adapters/newapi/parsers.rs
git commit -m "feat: normalize NewAPI collector payloads"
```

### Task 3: Make Cookie sessions first-class and persist login sources atomically

**Files:**
- Modify: `src-tauri/src/models/credentials.rs`
- Modify: `src-tauri/src/services/collectors/session.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add failing database tests for Cookie-only readiness and source preservation**

Add tests to the existing `database.rs` test module:

```rust
#[test]
fn newapi_session_cookie_only_is_ready() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "newapi-cookie-only");
    let data_key = [37_u8; 32];
    database.persist_station_session_with_data_key(
        PersistStationSessionInput {
            station_id: station.id.clone(),
            access_token: None,
            refresh_token: None,
            cookie: Some("session=encrypted-at-rest".to_string()),
            newapi_user_id: Some("42".to_string()),
            token_expires_at: None,
            session_expires_at: None,
            session_source: "password_login".to_string(),
        },
        &data_key,
    ).expect("persist session");
    let session = database.resolve_station_session_with_data_key(station.id, &data_key, 100_000)
        .expect("resolve session");
    assert_eq!(session.status, SessionResolveStatus::Ready);
    assert_eq!(session.cookie.as_deref(), Some("session=encrypted-at-rest"));
    assert_eq!(session.newapi_user_id.as_deref(), Some("42"));
}

#[test]
fn newapi_session_invalidating_cookie_keeps_manual_access_token() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "newapi-scoped-invalidation");
    let data_key = [41_u8; 32];
    persist_dual_newapi_session(&database, &station.id, &data_key);
    database.invalidate_station_session_credential(&station.id, StationSessionCredentialKind::Cookie)
        .expect("invalidate cookie");
    let credentials = database.get_station_credentials(station.id).expect("credentials");
    assert!(credentials.access_token_present);
    assert!(!credentials.cookie_present);
}

fn persist_dual_newapi_session(database: &AppDatabase, station_id: &str, data_key: &[u8; 32]) {
    database.persist_station_session_with_data_key(
        PersistStationSessionInput {
            station_id: station_id.to_string(),
            access_token: Some("manual-access-token".to_string()),
            refresh_token: None,
            cookie: Some("session=login-cookie".to_string()),
            newapi_user_id: Some("42".to_string()),
            token_expires_at: None,
            session_expires_at: None,
            session_source: "manual_token".to_string(),
        },
        data_key,
    ).expect("persist dual session");
}
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_session_
```

Expected: compilation fails after renaming the two tests with the `newapi_session_` prefix because the persistence input, credential kind, and methods do not exist.

- [ ] **Step 3: Add internal session types**

Add to `models/credentials.rs`:

```rust
#[derive(Debug, Clone)]
pub struct PersistStationSessionInput {
    pub station_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub token_expires_at: Option<String>,
    pub session_expires_at: Option<String>,
    pub session_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationSessionCredentialKind {
    AccessToken,
    RefreshToken,
    Cookie,
}
```

- [ ] **Step 4: Implement atomic persistence and scoped invalidation**

Add public `AppDatabase` methods that open one connection and call private connection helpers:

```rust
pub fn persist_station_session_with_data_key(
    &self,
    input: PersistStationSessionInput,
    data_key: &[u8; 32],
) -> Result<StationCredentials, String> {
    let mut connection = self.connection()?;
    validate_station_exists(&connection, &input.station_id)?;
    let station_id = input.station_id.clone();
    let transaction = connection.transaction()
        .map_err(|error| format!("开始保存 session 事务失败: {error}"))?;
    persist_station_session_from_connection(&transaction, input, data_key)?;
    transaction.commit()
        .map_err(|error| format!("提交保存 session 事务失败: {error}"))?;
    station_credentials_from_connection(&connection, &station_id)
}

pub fn invalidate_station_session_credential(
    &self,
    station_id: &str,
    kind: StationSessionCredentialKind,
) -> Result<(), String> {
    let mut connection = self.connection()?;
    validate_station_exists(&connection, station_id)?;
    let transaction = connection.transaction()
        .map_err(|error| format!("开始失效 session 事务失败: {error}"))?;
    invalidate_station_session_credential_from_connection(&transaction, station_id, kind)?;
    transaction.commit()
        .map_err(|error| format!("提交失效 session 事务失败: {error}"))
}
```

`persist_station_session_from_connection` must encrypt only supplied secrets, preserve other secret IDs, and write the caller-supplied `session_source`. `invalidate_station_session_credential_from_connection` must clear exactly one secret ID column, delete only that unreferenced encrypted secret row, and update session metadata without touching encrypted rows owned by other modes.

- [ ] **Step 5: Treat Cookie + user ID as Ready**

In `resolve_station_session_from_connection`, add this branch after a fresh access token and before refresh/password fallback:

```rust
if cookie.is_some() && newapi_user_id.as_deref().is_some_and(|value| !value.trim().is_empty()) {
    return Ok(ResolvedSession {
        status: SessionResolveStatus::Ready,
        access_token,
        refresh_token,
        cookie,
        newapi_user_id,
        message: None,
    });
}
```

- [ ] **Step 6: Verify GREEN and existing credential tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_session_
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast station_session
```

Expected: all matching tests pass, including the existing manual-token encryption test.

- [ ] **Step 7: Commit session persistence**

```powershell
git add -- src-tauri/src/models/credentials.rs src-tauri/src/services/collectors/session.rs src-tauri/src/services/database.rs
git commit -m "feat: persist NewAPI Cookie sessions"
```

### Task 4: Add bounded NewAPI authentication and HTTP execution

**Files:**
- Create: `src-tauri/src/services/collectors/adapters/newapi/auth.rs`
- Create: `src-tauri/src/services/collectors/adapters/newapi/client.rs`
- Create: `src-tauri/src/services/collectors/adapters/newapi/test_support.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Add failing auth/client tests using loopback servers**

Add tests that use `std::net::TcpListener::bind("127.0.0.1:0")` and assert:

```rust
#[test]
fn login_normalizes_multiple_set_cookie_headers() {
    let headers = vec![
        "session=abc; Path=/; HttpOnly; SameSite=Lax".to_string(),
        "lang=zh; Path=/".to_string(),
    ];
    assert_eq!(normalize_set_cookie_headers(&headers), Some("session=abc; lang=zh".to_string()));
}

#[test]
fn access_token_and_cookie_emit_distinct_headers() {
    assert_eq!(
        NewApiAuthContext::access_token("secret", "42").authorization_value(),
        Some("Bearer secret")
    );
    assert_eq!(
        NewApiAuthContext::cookie("session=abc", "42").cookie_value(),
        Some("session=abc")
    );
}

#[test]
fn get_retries_one_transient_failure_but_create_never_retries() {
    assert_eq!(NewApiOperation::ListTokens.max_transient_retries(), 1);
    assert_eq!(NewApiOperation::CreateToken.max_transient_retries(), 0);
    assert!(NewApiOperation::CreateToken.is_non_idempotent());
}
```

Add a loopback integration test where the first GET returns 502 and the second returns a valid envelope. Assert exactly two accepted connections. Add a create test where the server accepts one POST and closes without a response; assert the client returns `NewApiRequestError::OutcomeUnknown` and no second connection is accepted.

Create `test_support.rs` with this test-only interface so later tasks do not duplicate socket parsing:

```rust
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(super) struct TestHttpServer {
    pub base_url: String,
    pub requests: std::sync::mpsc::Receiver<String>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestHttpServer {
    pub fn sequence(raw_responses: Vec<Option<String>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        listener.set_nonblocking(true).expect("nonblocking fixture server");
        let address = listener.local_addr().expect("fixture address");
        let (sender, requests) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            for response in raw_responses {
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(accepted) => break accepted,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("fixture accept failed: {error}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_millis(200))).expect("read timeout");
                let mut bytes = [0_u8; 8192];
                let size = stream.read(&mut bytes).unwrap_or(0);
                sender.send(String::from_utf8_lossy(&bytes[..size]).to_string()).expect("capture request");
                if let Some(response) = response {
                    stream.write_all(response.as_bytes()).expect("write fixture response");
                }
            }
        });
        Self { base_url: format!("http://{address}"), requests, handle: Some(handle) }
    }

    pub fn finish(mut self) -> Vec<String> {
        self.handle.take().expect("fixture handle").join().expect("fixture thread");
        self.requests.try_iter().collect()
    }
}

pub(super) fn json_response(status: u16, body: serde_json::Value) -> String {
    let body = body.to_string();
    let reason = if status == 200 { "OK" } else { "ERROR" };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
}
```

`None` means accept one request and close the connection without writing a response. `finish` joins the server thread and drains captured raw requests. Bind only to `127.0.0.1:0`, set a 2-second nonblocking acceptance deadline, and never print captured requests.

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast collectors::adapters::newapi::
```

Expected: compilation fails because the modules and types do not exist.

- [ ] **Step 3: Define authentication and operation policies**

In `auth.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NewApiAuthKind { AccessToken, Cookie }

#[derive(Debug, Clone)]
pub(super) struct NewApiAuthContext {
    pub kind: NewApiAuthKind,
    pub secret: String,
    pub user_id: String,
}

impl NewApiAuthContext {
    pub fn access_token(secret: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self { kind: NewApiAuthKind::AccessToken, secret: secret.into(), user_id: user_id.into() }
    }
    pub fn cookie(secret: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self { kind: NewApiAuthKind::Cookie, secret: secret.into(), user_id: user_id.into() }
    }
    pub fn authorization_value(&self) -> Option<String> {
        (self.kind == NewApiAuthKind::AccessToken).then(|| format!("Bearer {}", self.secret))
    }
    pub fn cookie_value(&self) -> Option<&str> {
        (self.kind == NewApiAuthKind::Cookie).then_some(self.secret.as_str())
    }
}
```

In `client.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NewApiOperation {
    Status,
    SelfInfo,
    Groups,
    Models,
    ListTokens,
    RevealToken,
    CreateToken,
}

#[derive(Debug, Clone)]
pub(super) struct NewApiResponse {
    pub data: serde_json::Value,
    pub endpoint_result: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NewApiRequestError {
    AuthRequired { code: String, message: String },
    ManualRequired { code: String, message: String },
    Transient { code: String, message: String },
    OutcomeUnknown { code: String, message: String },
    Permanent { code: String, message: String },
}

impl NewApiOperation {
    pub fn max_transient_retries(self) -> usize {
        match self { Self::CreateToken => 0, _ => 1 }
    }
    pub fn is_non_idempotent(self) -> bool { self == Self::CreateToken }
}
```

Use constants for the 20-second timeout, one auth retry, and per-operation transient budget.

- [ ] **Step 4: Implement login and Cookie normalization**

`login_with_password` must POST a JSON object whose `username` value is `login_username` and whose `password` value is `login_password`, parse `data.id`, gather every `Set-Cookie` header, normalize only name/value pairs, and call `persist_station_session_with_data_key` with `session_source = "password_login"`. A response with `require_2fa: true` or a challenge message returns a stable `manual_session_required` error without retrying.

- [ ] **Step 5: Implement authenticated request recovery**

The request executor must:

```rust
pub(super) fn get_authenticated_json(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    path: &str,
    operation: NewApiOperation,
) -> Result<NewApiResponse, NewApiRequestError>;
```

Resolve access token, then Cookie, then password login. On confirmed auth failure, invalidate only the used credential kind, resolve the next path, and retry authentication once. On network/408/429/5xx, obey `max_transient_retries`. Redact and truncate every returned message before adding endpoint diagnostics.

- [ ] **Step 6: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast collectors::adapters::newapi::
```

Expected: Cookie normalization, header selection, auth fallback, one-shot GET retry, and no-retry create tests pass.

- [ ] **Step 7: Commit auth and client**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/adapters/newapi/auth.rs src-tauri/src/services/collectors/adapters/newapi/client.rs src-tauri/src/services/collectors/adapters/newapi/test_support.rs
git commit -m "feat: add bounded NewAPI authentication"
```

### Task 5: Implement Detect, Balance, Groups, Models, and Full collection

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Test: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Add failing adapter-output tests**

Use a loopback server and in-memory database to assert:

```rust
#[test]
fn models_snapshot_keeps_top_level_models_contract() {
    let output = build_models_output(
        "station-1",
        parsers::envelope_data(&json!({"success": true, "data": ["gpt-4.1-mini", "claude-sonnet"]}))
            .expect("model data"),
        json!({"path": "/api/user/models", "status": 200, "ok": true}),
    );
    assert_eq!(output.normalized_json["models"], json!(["gpt-4.1-mini", "claude-sonnet"]));
    assert_eq!(output.facts.models.len(), 2);
}

#[test]
fn empty_successful_group_payload_is_partial() {
    let output = build_groups_output(
        "station-1",
        parsers::envelope_data(&json!({"success": true, "data": {}})).expect("group data"),
        json!({"path": "/api/user/self/groups", "status": 200, "ok": true}),
    );
    assert_eq!(output.status, "partial");
    assert_eq!(output.error_code.as_deref(), Some("empty_group_facts"));
}
```

Add a Full composition test asserting NewAPI child task order is Balance, Groups, Models.

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_
```

Expected: the new output and Full task tests fail.

- [ ] **Step 3: Replace fixed Detect with `/api/status`**

Build Detect output with a real endpoint diagnostic and normalized fields:

```rust
normalized_json: json!({
    "adapter": "newapi",
    "systemName": status.system_name,
    "quotaPerUnit": status.quota_per_unit,
    "quotaDisplayType": status.quota_display_type,
    "models": [],
})
```

- [ ] **Step 4: Implement Balance, Groups, and Models**

Balance must fetch status during every Balance task, then self info, and mark the 500000 fallback in summary diagnostics. Groups must preserve object-map keys. Models must put names in both `CollectorFacts.models` and `normalized_json.models`. A successful envelope with zero critical facts returns `partial` with a stable error code instead of `success`.

Keep output construction pure and testable with these module-local signatures:

```rust
fn build_groups_output(
    station_id: &str,
    data: &serde_json::Value,
    endpoint_result: serde_json::Value,
) -> AdapterOutput;

fn build_models_output(
    station_id: &str,
    data: &serde_json::Value,
    endpoint_result: serde_json::Value,
) -> AdapterOutput;
```

- [ ] **Step 5: Add Models to NewAPI Full composition**

Change `full_child_tasks`:

```rust
"newapi" => vec![
    adapters::CollectorTask::Balance,
    adapters::CollectorTask::Groups,
    adapters::CollectorTask::Models,
],
"sub2api" => vec![
    adapters::CollectorTask::Balance,
    adapters::CollectorTask::Groups,
],
```

- [ ] **Step 6: Verify collection tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast full_snapshot_summarizes_child_business_facts
```

Expected: NewAPI parser/output tests and existing Full aggregation tests pass.

- [ ] **Step 7: Commit collection tasks**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/mod.rs
git commit -m "feat: collect NewAPI station facts"
```

### Task 6: Implement all-or-nothing remote-Key scan and explicit secret reveal

**Files:**
- Create: `src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Test: `src-tauri/src/services/remote_keys.rs`

- [ ] **Step 1: Add failing pagination and preservation tests**

Add tests with a two-page loopback server:

```rust
#[test]
fn newapi_token_scan_reads_every_page_before_replace() {
    let tokens = scan_tokens_from_pages(vec![
        json!({"page": 1, "page_size": 1, "total": 2, "items": [masked_token(10, "first")]}),
        json!({"page": 2, "page_size": 1, "total": 2, "items": [masked_token(11, "second")]}),
    ]).expect("scan");
    assert_eq!(tokens.iter().map(|token| token.id.as_str()).collect::<Vec<_>>(), vec!["10", "11"]);
}

#[test]
fn failed_second_page_keeps_previous_remote_discoveries() {
    let database = seeded_remote_key_database();
    let error = run_newapi_scan_with_failed_second_page(&database).unwrap_err();
    assert!(error.contains("pagination_incomplete"));
    assert_eq!(database.list_remote_station_keys("station-1".to_string()).unwrap().len(), 1);
}
```

Define the named helpers in the same test module with these exact signatures:

```rust
fn masked_token(id: i64, name: &str) -> serde_json::Value;
fn scan_tokens_from_pages(pages: Vec<serde_json::Value>) -> Result<Vec<RemoteStationKey>, String>;
fn seeded_remote_key_database() -> AppDatabase;
fn run_newapi_scan_with_failed_second_page(database: &AppDatabase) -> Result<RemoteKeyScanResult, String>;
```

`masked_token` returns every field observed in the NewAPI list contract with a masked `key`. `seeded_remote_key_database` creates one NewAPI station and stores one prior remote discovery through `replace_remote_station_keys`. `run_newapi_scan_with_failed_second_page` must call the production `services::remote_keys::scan_remote_keys`, not a parser-only shortcut.

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_token_scan_
```

Expected: tests fail because NewAPI scanning is unsupported.

- [ ] **Step 3: Implement safe pagination**

Use page size 100 and a hard item cap constant. Reject repeated pages, changing totals, missing items, count/total mismatch, and cap exhaustion. Return `Err` before `services::remote_keys::scan_remote_keys` reaches `replace_remote_station_keys`; never return a partial `Vec<RemoteStationKey>`.

Change the NewAPI capability returned from the adapter to:

```rust
RemoteKeyCapability {
    station_id: station.id.clone(),
    station_type: "newapi".to_string(),
    can_list_remote_keys: true,
    can_create_remote_key: true,
    can_read_groups: true,
    requires_manual_session: true,
    unsupported_reason: None,
}
```

Map token ID, name, masked key, group, created/accessed timestamps, and redacted source. Do not call the reveal endpoint during scan.

Keep pagination parsing explicit:

```rust
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ParsedTokenPage {
    pub page: usize,
    pub page_size: usize,
    pub total: usize,
    pub items: Vec<RemoteStationKey>,
}

pub(super) fn parse_token_page(
    station_id: &str,
    data: &serde_json::Value,
) -> Result<ParsedTokenPage, String>;
```

- [ ] **Step 4: Implement explicit reveal dispatch**

Add to `remote_key_full_secret_with_adapter`:

```rust
"newapi" => adapters::newapi::scan_remote_key_full_secret(
    database,
    data_key,
    station_id,
    remote_key_id,
),
```

The adapter must POST `/api/token/{id}/key`, verify the returned ID against the requested discovery, extract the full key in Rust, and return it only to `save_created_remote_key`.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_token_scan_
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast local_key_from_remote_discovery_uses_full_secret_not_mask_placeholder
```

Expected: pagination, preservation, and full-secret local-save tests pass.

- [ ] **Step 6: Commit scan and reveal**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs src-tauri/src/services/remote_keys.rs
git commit -m "feat: scan NewAPI remote keys"
```

### Task 7: Create remote Keys without duplicate retries or IPC secret leakage

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Test: `src-tauri/src/services/remote_keys.rs`

- [ ] **Step 1: Add failing create-state and secret-return tests**

```rust
#[test]
fn newapi_create_reconciles_one_new_matching_token() {
    let before = vec![remote_token_id(10)];
    let after = vec![remote_token_id(10), remote_token(11, "relay-test", "default")];
    assert_eq!(reconcile_created_token(&before, &after, "relay-test", "default").unwrap().id, 11);
}

#[test]
fn newapi_create_rejects_ambiguous_candidates() {
    let before = vec![remote_token_id(10)];
    let after = vec![
        remote_token(11, "same", "default"),
        remote_token(12, "same", "default"),
    ];
    assert_eq!(
        reconcile_created_token(&before, &after, "same", "default").unwrap_err().code,
        "create_candidate_ambiguous"
    );
}

#[test]
fn create_remote_key_result_never_returns_full_secret() {
    let result = create_and_save_remote_key_for_test().expect("create result");
    assert!(result.station_key.api_key_present);
    assert!(result.full_key_once.is_none());
}
```

Define the test helpers in the same module:

```rust
fn remote_token_id(id: i64) -> ParsedRemoteToken;
fn remote_token(id: i64, name: &str, group: &str) -> ParsedRemoteToken;
fn create_and_save_remote_key_for_test() -> Result<CreateRemoteStationKeyResult, String>;
```

Use this production reconciliation type and signature:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRemoteToken {
    id: i64,
    name: String,
    group: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NewApiCreateError {
    code: String,
    message: String,
}

fn reconcile_created_token(
    before: &[ParsedRemoteToken],
    after: &[ParsedRemoteToken],
    expected_name: &str,
    expected_group: &str,
) -> Result<ParsedRemoteToken, NewApiCreateError>;
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_create_
```

Expected: reconciliation tests fail and the current shared service test observes `full_key_once.is_some()`.

- [ ] **Step 3: Implement the NewAPI create policy**

Create request JSON must be exactly:

```rust
json!({
    "name": input.name,
    "group": group_name,
    "expired_time": -1,
    "unlimited_quota": true,
    "remain_quota": 0,
    "model_limits_enabled": false,
    "model_limits": "",
    "allow_ips": "",
    "cross_group_retry": false,
})
```

Read the complete before-ID set, send one create POST, then perform one read-only scan. On a timeout/closed connection, use the same reconciliation scan and surface `create_outcome_unknown` when zero or multiple candidates remain. Never resend the create POST.

- [ ] **Step 4: Suppress `fullKeyOnce` after encryption**

In both returns from `save_created_remote_key`, use:

```rust
full_key_once: None,
```

The full key must remain a local variable passed directly to `create_station_key_with_data_key`; it must not be cloned into messages, snapshots, or command results.

- [ ] **Step 5: Verify GREEN and Sub2API compatibility**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_create_
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast remote_key
```

Expected: create reconciliation, secret suppression, and all existing Sub2API remote-key service tests pass.

- [ ] **Step 6: Commit create workflow**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs src-tauri/src/services/remote_keys.rs
git commit -m "feat: create NewAPI remote keys safely"
```

### Task 8: Dispatch login tests by station type

**Files:**
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/auth.rs`
- Test: `src-tauri/src/services/collectors/mod.rs`

- [ ] **Step 1: Add a failing NewAPI login-dispatch test**

```rust
#[test]
fn test_station_login_dispatches_newapi_password_login() {
    let server = newapi_login_fixture_server();
    let database = database_with_saved_newapi_credentials(server.base_url());
    let result = test_station_login(&database, &[23_u8; 32], server.station_id())
        .expect("login result");
    assert_eq!(result.snapshot.status, "success");
    let credentials = database.get_station_credentials(server.station_id()).expect("credentials");
    assert!(credentials.cookie_present);
    assert_eq!(credentials.newapi_user_id.as_deref(), Some("42"));
    assert_eq!(credentials.session_source, "password_login");
}
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast test_station_login_dispatches_newapi_password_login
```

Expected: the assertion fails because `test_station_login` calls the Sub2API login probe.

- [ ] **Step 3: Add station-type dispatch**

Add a neutral private orchestration result in `collectors/mod.rs`; do not reuse Sub2API's `token_present` wording for a successful NewAPI Cookie login:

```rust
struct LoginTestOutcome {
    succeeded: bool,
    message: Option<String>,
    manual_required: Option<String>,
}
```

Replace the hard-coded login call with:

```rust
let login_outcome = match station.station_type.trim() {
    "sub2api" => {
        let outcome = sub2api::test_login_credentials(&station.base_url, &login_username, &login_password)?;
        LoginTestOutcome {
            succeeded: outcome.token_present,
            message: outcome.login_message,
            manual_required: outcome.manual_required,
        }
    }
    "newapi" => {
        let outcome = adapters::newapi::test_login_credentials(
            database,
            data_key,
            &station,
            &login_username,
            &login_password,
        )?;
        LoginTestOutcome {
            succeeded: outcome.cookie_present,
            message: outcome.login_message,
            manual_required: outcome.manual_required,
        }
    }
    _ => return Ok(build_status_result(
        station_id,
        station.name,
        "unsupported_login_test",
        "该站点类型不支持账号密码登录测试。",
        "请使用手动 API Key 或登录态。",
    )),
};
```

Define the NewAPI return type in `auth.rs`:

```rust
pub(crate) struct NewApiLoginProbeOutcome {
    pub cookie_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
}
```

Use `login_outcome.succeeded`, `login_outcome.message`, and `login_outcome.manual_required` for the existing snapshot/event contract. Change labels and diagnosis text only where they currently claim every login uses a token.

- [ ] **Step 4: Verify GREEN and existing login tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast test_station_login
```

Expected: the new NewAPI dispatch test and existing saved-password Sub2API tests pass.

- [ ] **Step 5: Commit login dispatch**

```powershell
git add -- src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/adapters/newapi/auth.rs
git commit -m "feat: test NewAPI station logins"
```

### Task 9: Make the frontend truthful and confirm NewAPI create policy

**Files:**
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Create: `scripts/newapi-collector-contract.test.mjs`

- [ ] **Step 1: Add a failing source-contract test**

Create the script:

```javascript
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const collector = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const provider = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const remoteService = await readFile("src-tauri/src/services/remote_keys.rs", "utf8");

assert.ok(!collector.includes("NewAPI 采集（待接入）"), "NewAPI collector copy must be truthful");
assert.ok(
  provider.includes("永久、无限额度") && provider.includes("ConfirmDialog"),
  "NewAPI remote-key creation must disclose its compatibility defaults",
);
assert.ok(
  remoteService.includes("full_key_once: None"),
  "remote-key command results must not return full secrets over IPC",
);

console.log("newapi collector contract checks passed");
```

- [ ] **Step 2: Run RED**

Run:

```powershell
node .\scripts\newapi-collector-contract.test.mjs
```

Expected: FAIL because the waiting copy and create confirmation are still missing.

- [ ] **Step 3: Update Collector UI wording and states**

Change `adapterForStation` to return `"NewAPI 登录态采集"`. Keep Balance, Groups, and Models as existing `CollectorTaskType` options. Show remote-Key refresh as an event/discovery result, not a fabricated task type.

- [ ] **Step 4: Add the NewAPI create confirmation**

Before calling `createRemoteStationKey`, set pending creation state for NewAPI stations and render the established `ConfirmDialog`:

```tsx
<ConfirmDialog
  open={pendingNewApiRemoteCreate !== null}
  title="创建 NewAPI 远程 Key"
  description="将创建永久、无限额度、无模型限制和无 IP 限制的远程 Key。创建后会立即加密保存为本地 Key。"
  confirmLabel="创建并保存"
  confirming={creatingRemoteKey}
  onCancel={() => setPendingNewApiRemoteCreate(null)}
  onConfirm={() => void confirmCreateNewApiRemoteKey()}
/>
```

Sub2API continues through its existing path without this NewAPI-specific policy confirmation.

- [ ] **Step 5: Remove browser-preview full-secret fabrication**

In the `createRemoteStationKey` preview fallback, return `fullKeyOnce: null`. Keep the mock StationKey's `apiKeyPresent` behavior without exposing or returning its generated preview secret.

- [ ] **Step 6: Run GREEN and related frontend contracts**

Run:

```powershell
node .\scripts\newapi-collector-contract.test.mjs
node .\scripts\add-provider-key-groups.test.mjs
node .\scripts\station-auto-collector.test.mjs
pnpm.cmd build
```

Expected: all three scripts print their pass messages and TypeScript/Vite build exits 0.

- [ ] **Step 7: Commit frontend truthfulness**

```powershell
git add -- src/features/collectors/CollectorsPage.tsx src/features/stations/AddProviderPage.tsx src/lib/api/stationKeys.ts src/lib/types/stationKeys.ts scripts/newapi-collector-contract.test.mjs
git commit -m "feat: expose NewAPI collector capability"
```

### Task 10: Run the complete offline regression suite

**Files:**
- No source changes expected

- [ ] **Step 1: Run focused Rust tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast newapi_
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast remote_key
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast station_session
```

Expected: zero failed tests in all commands.

- [ ] **Step 2: Run full Rust validation**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 3: Run frontend and source-contract validation**

```powershell
node .\scripts\newapi-collector-contract.test.mjs
node .\scripts\station-auto-collector.test.mjs
node .\scripts\add-provider-key-groups.test.mjs
pnpm.cmd build
```

Expected: contract scripts pass and the production build exits 0.

- [ ] **Step 4: Audit secrets and scope**

```powershell
rg -n '(?i)(password|cookie|access[_-]?token|authorization)\s*[:=]\s*["''][^"'']+["'']' src src-tauri scripts docs/superpowers
git diff --check
git status --short
```

Expected: every `rg` hit is an explicitly synthetic fixture or public header name; no real credential value is present. `git diff --check` reports no errors; status contains only intentional implementation paths.

- [ ] **Step 5: Verify the already-integrated collector-failure behavior**

The execution base already contains the collector-failure transition work. Run its focused tests after adding the NewAPI session helpers to the same `database.rs`:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib --no-fail-fast collector_failure
```

Expected: repeated failure, recovery, and task-scoping tests pass with zero failures.

### Task 11: Add and run an opt-in live lifecycle test last

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Add a default-skipped live test**

Inside the NewAPI adapter test module, add:

```rust
struct LiveNewApiHarness {
    database: AppDatabase,
    data_key: [u8; 32],
    station_id: String,
}

impl LiveNewApiHarness {
    fn new_isolated(base_url: String, username: String, password: String) -> Result<Self, String>;
    fn collect_read_only(&self) -> Result<(usize, usize, usize), String>;
    fn create_remote_for_cleanup(&self, name: &str, group: &str) -> Result<String, String>;
    fn reveal_and_import(&self, remote_id: &str) -> Result<String, String>;
    fn verify_local_secret_is_encrypted(&self, local_key_id: &str) -> Result<(), String>;
    fn delete_remote_token(&self, remote_id: &str) -> Result<(), String>;
}

#[test]
#[ignore = "requires explicit NewAPI credentials"]
fn newapi_live_read_only() {
    let harness = LiveNewApiHarness::new_isolated(
        std::env::var("NEWAPI_E2E_BASE_URL").expect("NEWAPI_E2E_BASE_URL"),
        std::env::var("NEWAPI_E2E_USERNAME").expect("NEWAPI_E2E_USERNAME"),
        std::env::var("NEWAPI_E2E_PASSWORD").expect("NEWAPI_E2E_PASSWORD"),
    ).expect("isolated live harness");
    let (group_count, model_count, remote_key_count) = harness.collect_read_only()
        .expect("read-only collection");
    assert!(group_count > 0);
    assert!(model_count > 0);
    assert!(remote_key_count > 0);
}

#[test]
#[ignore = "requires explicit NewAPI credentials and remote write permission"]
fn newapi_live_temp_key_lifecycle() {
    assert_eq!(std::env::var("NEWAPI_E2E_ALLOW_WRITE").as_deref(), Ok("1"));
    let base_url = std::env::var("NEWAPI_E2E_BASE_URL").expect("NEWAPI_E2E_BASE_URL");
    let username = std::env::var("NEWAPI_E2E_USERNAME").expect("NEWAPI_E2E_USERNAME");
    let password = std::env::var("NEWAPI_E2E_PASSWORD").expect("NEWAPI_E2E_PASSWORD");
    let test_name = format!("relay-pool-e2e-{}", now_millis_for_services());
    let harness = LiveNewApiHarness::new_isolated(base_url, username, password)
        .expect("isolated live harness");
    let cleanup_id = harness.create_remote_for_cleanup(&test_name, "default")
        .expect("create temporary remote key");
    let verification = harness.reveal_and_import(&cleanup_id)
        .and_then(|local_key_id| harness.verify_local_secret_is_encrypted(&local_key_id));
    let cleanup = harness.delete_remote_token(&cleanup_id);
    assert!(cleanup.is_ok(), "temporary remote token cleanup failed for id {cleanup_id}");
    verification.expect("local encrypted secret verification");
}
```

Implement every method above by calling production adapter/service entry points. `LiveNewApiHarness::new_isolated` must use a temporary data directory and in-memory/test database. `delete_remote_token` is test-only code under `#[cfg(test)]`; do not expose remote delete through Tauri commands or frontend APIs. After remote creation, store the cleanup ID before reveal/import and avoid `expect` or `assert` until deletion has been attempted. The test must never print the password, Cookie, full Key, or request/response bodies.

- [ ] **Step 2: Prove the live test is skipped by default**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib newapi_live_temp_key_lifecycle
```

Expected: one ignored test, zero failures, and no network mutation.

- [ ] **Step 3: Run live read-only collection first**

Set the four `NEWAPI_E2E_*` environment variables outside the repository and shell history, leaving `NEWAPI_E2E_ALLOW_WRITE` unset. Run the read-only live test filter supplied by the harness:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib newapi_live_read_only -- --ignored --nocapture
```

Expected: login, balance, groups, models, and masked remote-Key listing pass; output contains counts and statuses only.

- [ ] **Step 4: Run the authorized write lifecycle once**

After read-only success, set `NEWAPI_E2E_ALLOW_WRITE=1` in the current process and run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib newapi_live_temp_key_lifecycle -- --ignored --nocapture
```

Expected: one temporary remote Key is created, uniquely reconciled, revealed only inside Rust, encrypted into the isolated local database, and deleted remotely. The test exits 0 without printing secrets.

- [ ] **Step 5: Handle cleanup failure as a hard stop**

If the write test reports cleanup failure, stop. Use the reported numeric remote token ID with the authenticated NewAPI delete endpoint to remove exactly that test token, then rerun only the remote list check. Do not create another test token until cleanup is confirmed.

- [ ] **Step 6: Commit the opt-in harness after successful offline verification**

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/adapters/newapi/remote_keys.rs
git commit -m "test: cover live NewAPI key lifecycle"
```

- [ ] **Step 7: Final verification and closeout**

```powershell
git status --short
git log --oneline -12
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
pnpm.cmd build
```

Expected: only explicitly acknowledged unrelated worktree state remains, the planned commits are present, all Rust library tests pass, and the frontend production build succeeds.

## Spec Coverage Audit

| Approved design requirement | Implementation task |
|---|---|
| Independent NewAPI module boundaries | Tasks 1, 2, 4, 6 |
| Access token, Cookie, and password-login compatibility | Tasks 3, 4, 8 |
| Cookie normalization and atomic encrypted persistence | Tasks 3, 4 |
| Detect and dynamic `quota_per_unit` | Tasks 2, 5 |
| Balance, group-map identity, and raw multipliers | Tasks 2, 5 |
| Models in `CollectorFacts` and `normalized_json.models` | Tasks 2, 5 |
| NewAPI Full child tasks and failure isolation | Task 5 |
| Remote capability, all-or-nothing pagination, and history preservation | Task 6 |
| Explicit full-secret reveal without list-time exposure | Task 6 |
| Non-idempotent create reconciliation | Task 7 |
| `fullKeyOnce` IPC suppression | Tasks 7, 9 |
| Truthful existing UI and high-privilege create confirmation | Task 9 |
| Bounded retries and stable error semantics | Tasks 4, 5, 6, 7 |
| AGPL implementation boundary | Execution Preconditions, Tasks 2-7 |
| Offline regression and secret audit | Task 10 |
| Isolated, opt-in, cleanup-guaranteed real-site acceptance | Task 11 |
