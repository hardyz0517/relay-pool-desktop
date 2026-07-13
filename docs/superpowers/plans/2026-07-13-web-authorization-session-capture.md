# Web Authorization Session Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reliable, extensible Web authorization flow that lets NewAPI/KamiAPI-style stations complete login in a Tauri WebView and persist a verified collector session without relying on username/password login.

**Architecture:** Add a small Web authorization layer beside the existing capture service. The capture window still records safe HTTP evidence, but session persistence is moved to a verified Rust-side flow: read HttpOnly cookies from the capture WebView, validate them against the station's user endpoint, extract the NewAPI user id, then persist an encrypted session with a clear `session_source`. NewAPI collection then consumes the session through the existing credential resolver and treats expired Web sessions as manual re-authorization instead of attempting password login.

**Tech Stack:** Tauri 2 WebView windows, Rust services under `src-tauri/src/services`, rusqlite-backed encrypted station credentials, React/TypeScript collector UI, focused Rust unit tests plus existing frontend build/type checks.

---

## File Structure

Modify:
- `src-tauri/src/services/capture/mod.rs` - keep generic event redaction and field extraction, add user id extraction helpers used by Web authorization.
- `src-tauri/src/services/capture/session.rs` - keep capture session store unchanged unless status needs a new terminal value.
- `src-tauri/src/commands/mod.rs` - wire the capture-window commands to the new Web authorization service; keep command code thin.
- `src-tauri/src/services/collectors/adapters/newapi/client.rs` - resolve Web authorization sessions without falling back to password login after auth rejection.
- `src-tauri/src/services/collectors/adapters/newapi/auth.rs` - expose small parsing helpers only if the new service needs shared NewAPI envelope logic.
- `src/features/collectors/CollectorsPage.tsx` - rename and clarify the manual Web authorization UI state.
- `src/lib/api/collector.ts` - expose the new finish command result while preserving fallback behavior for non-Tauri previews.
- `src/lib/types/collector.ts` - extend capture status/result types with Web authorization details.
- `src/features/stations/providerPresets.ts` - add KamiAPI preset only after the generic flow is working.

Create:
- `src-tauri/src/services/capture/web_authorization.rs` - Web authorization verifier, cookie-header builder, NewAPI user self probe, session persistence input builder.
- `scripts/web-authorization-session-source.test.mjs` - frontend/source guard for the visible "网页登录授权" surface and no stale "实验" copy.

Test:
- `src-tauri/src/services/capture/web_authorization.rs` unit tests.
- Existing `src-tauri/src/services/capture/mod.rs` unit tests, extended for `data.id`.
- Existing NewAPI client tests in `src-tauri/src/services/collectors/adapters/newapi/client.rs` or `mod.rs`.
- Existing collector UI build checks.

---

### Task 1: Extract Web Authorization User-Id Parsing

**Files:**
- Modify: `src-tauri/src/services/capture/mod.rs`
- Test: `src-tauri/src/services/capture/mod.rs`

- [ ] **Step 1: Add failing tests for NewAPI envelope user id extraction**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/capture/mod.rs`:

```rust
#[test]
fn capture_extracts_newapi_user_id_from_data_id() {
    let payload = json!({
        "success": true,
        "data": {
            "id": 42,
            "username": "kami-user"
        }
    });

    assert_eq!(
        super::extract_newapi_user_id(&payload).as_deref(),
        Some("42")
    );
}

#[test]
fn capture_extracts_newapi_user_id_from_nested_user_id() {
    let payload = json!({
        "data": {
            "profile": {
                "userId": "newapi-user-99"
            }
        }
    });

    assert_eq!(
        super::extract_newapi_user_id(&payload).as_deref(),
        Some("newapi-user-99")
    );
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml capture_extracts_newapi_user_id -- --nocapture
```

Expected: FAIL because `extract_newapi_user_id` does not exist.

- [ ] **Step 3: Add the parser helper**

Add this helper near `find_string_field` in `src-tauri/src/services/capture/mod.rs`:

```rust
pub(crate) fn extract_newapi_user_id(value: &Value) -> Option<String> {
    find_string_or_i64_field(
        value,
        &[
            "newapi_user_id",
            "newapiUserId",
            "user_id",
            "userId",
            "id",
        ],
    )
}

fn find_string_or_i64_field(value: &Value, names: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for name in names {
                if let Some(text) = map.get(*name).and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                if let Some(number) = map.get(*name).and_then(Value::as_i64) {
                    return Some(number.to_string());
                }
            }
            map.values()
                .find_map(|child| find_string_or_i64_field(child, names))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_string_or_i64_field(child, names)),
        _ => None,
    }
}
```

- [ ] **Step 4: Reuse helper in session extraction**

In `extract_session_credentials`, replace the `newapi_user_id` field expression with:

```rust
newapi_user_id: extract_newapi_user_id(json),
```

- [ ] **Step 5: Run capture tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml capture_ -- --nocapture
```

Expected: PASS, including the new user-id tests and existing capture redaction/session tests.

- [ ] **Step 6: Commit**

Use exact-path staging:

```powershell
git add src-tauri/src/services/capture/mod.rs
git commit -m "test: cover newapi web authorization user ids"
```

---

### Task 2: Add Cookie Header Builder and Web Authorization Types

**Files:**
- Create: `src-tauri/src/services/capture/web_authorization.rs`
- Modify: `src-tauri/src/services/capture/mod.rs`
- Test: `src-tauri/src/services/capture/web_authorization.rs`

- [ ] **Step 1: Create failing tests for cookie header normalization**

Create `src-tauri/src/services/capture/web_authorization.rs` with tests first:

```rust
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebAuthorizationSession {
    pub cookie_header: String,
    pub newapi_user_id: String,
}

pub(crate) fn build_cookie_header_from_pairs(pairs: &[(String, String)]) -> Option<String> {
    let mut parts = Vec::new();
    for (name, value) in pairs {
        let name = name.trim();
        let value = value.trim();
        if !name.is_empty() && !value.is_empty() {
            parts.push(format!("{name}={value}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub(crate) fn extract_verified_user_id(payload: &Value) -> Option<String> {
    super::extract_newapi_user_id(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_cookie_header_from_non_empty_pairs() {
        let pairs = vec![
            ("session".to_string(), "abc".to_string()),
            ("".to_string(), "ignored".to_string()),
            ("theme".to_string(), "light".to_string()),
        ];

        assert_eq!(
            build_cookie_header_from_pairs(&pairs).as_deref(),
            Some("session=abc; theme=light")
        );
    }

    #[test]
    fn extracts_verified_user_id_from_self_payload() {
        let payload = json!({
            "success": true,
            "data": {
                "id": 17
            }
        });

        assert_eq!(extract_verified_user_id(&payload).as_deref(), Some("17"));
    }
}
```

- [ ] **Step 2: Register the module and run RED**

Add this line to `src-tauri/src/services/capture/mod.rs`:

```rust
pub mod web_authorization;
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
```

Expected: PASS for the initial pure helpers. This task is allowed to start GREEN because it creates an isolated pure module with tests in the same step; later tasks introduce failing integration tests before behavior changes.

- [ ] **Step 3: Replace the placeholder session struct with final type names**

Keep the same file and replace `WebAuthorizationSession` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedWebAuthorizationSession {
    pub cookie_header: String,
    pub newapi_user_id: String,
    pub session_source: String,
}

impl VerifiedWebAuthorizationSession {
    pub(crate) fn new(cookie_header: String, newapi_user_id: String) -> Self {
        Self {
            cookie_header,
            newapi_user_id,
            session_source: "web_authorization".to_string(),
        }
    }
}
```

- [ ] **Step 4: Add final test for session source**

Add:

```rust
#[test]
fn verified_web_authorization_session_uses_stable_source() {
    let session = VerifiedWebAuthorizationSession::new(
        "session=abc".to_string(),
        "42".to_string(),
    );

    assert_eq!(session.session_source, "web_authorization");
}
```

- [ ] **Step 5: Run focused tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/services/capture/mod.rs src-tauri/src/services/capture/web_authorization.rs
git commit -m "feat: add web authorization session primitives"
```

---

### Task 3: Verify Cookie Session Against NewAPI Self Endpoint

**Files:**
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Test: `src-tauri/src/services/capture/web_authorization.rs`

- [ ] **Step 1: Write failing tests for verified `/api/user/self` behavior**

Add a small TCP fixture test module in `web_authorization.rs`:

```rust
#[cfg(test)]
mod verification_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    fn serve_once(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            stream.write_all(response.as_bytes()).expect("write response");
        });
        format!("http://{address}")
    }

    #[test]
    fn verifies_cookie_session_with_newapi_self_endpoint() {
        let base_url = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 42\r\nConnection: close\r\n\r\n{\"success\":true,\"data\":{\"id\":42,\"quota\":1}}",
        );

        let verified = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            std::time::Duration::from_secs(5),
        )
        .expect("verified session");

        assert_eq!(verified.newapi_user_id, "42");
        assert_eq!(verified.cookie_header, "session=abc");
        assert_eq!(verified.session_source, "web_authorization");
    }

    #[test]
    fn rejects_cookie_session_without_user_id() {
        let base_url = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"success\":true,\"data\":{\"quota\":1}}",
        );

        let error = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            std::time::Duration::from_secs(5),
        )
        .expect_err("missing user id should fail");

        assert!(error.contains("user id"));
    }
}
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml verifies_cookie_session_with_newapi_self_endpoint -- --nocapture
```

Expected: FAIL because `verify_newapi_cookie_session` does not exist.

- [ ] **Step 3: Implement verifier**

Add this implementation to `web_authorization.rs`:

```rust
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::services::collectors::url::join_url;

pub(crate) fn verify_newapi_cookie_session(
    management_base_url: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<VerifiedWebAuthorizationSession, String> {
    let cookie_header = cookie_header.trim();
    if cookie_header.is_empty() {
        return Err("Web authorization did not capture a usable Cookie header.".to_string());
    }

    let url = join_url(management_base_url, "/api/user/self");
    let started = Instant::now();
    let response = ureq::get(&url)
        .timeout(timeout)
        .set("Cookie", cookie_header)
        .set("Content-Type", "application/json")
        .call()
        .map_err(|error| format!("Web authorization self probe failed: {error}"))?;

    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(format!(
            "Web authorization self probe returned HTTP {status} after {} ms.",
            started.elapsed().as_millis()
        ));
    }

    let payload = serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("Web authorization self probe returned invalid JSON: {error}"))?;
    let user_id = extract_verified_user_id(&payload)
        .ok_or_else(|| "Web authorization self probe did not return a user id.".to_string())?;

    Ok(VerifiedWebAuthorizationSession::new(
        cookie_header.to_string(),
        user_id,
    ))
}
```

- [ ] **Step 4: Run verifier tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/capture/web_authorization.rs
git commit -m "feat: verify web authorization sessions"
```

---

### Task 4: Read Capture WebView Cookies Safely

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Test: `src-tauri/src/services/capture/web_authorization.rs`

- [ ] **Step 1: Add pure test for converting Tauri cookie pairs**

Keep the WebView API call in `commands/mod.rs`, but add this pure test in `web_authorization.rs`:

```rust
#[test]
fn cookie_pairs_ignore_empty_names_and_values() {
    let pairs = vec![
        ("".to_string(), "abc".to_string()),
        ("session".to_string(), "".to_string()),
        ("session".to_string(), "abc".to_string()),
    ];

    assert_eq!(
        build_cookie_header_from_pairs(&pairs).as_deref(),
        Some("session=abc")
    );
}
```

- [ ] **Step 2: Run test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml cookie_pairs_ignore_empty_names_and_values -- --nocapture
```

Expected: PASS once the existing helper handles empty fields.

- [ ] **Step 3: Add helper in commands for capture window cookie extraction**

In `src-tauri/src/commands/mod.rs`, add this helper near `capture_window_label`:

```rust
async fn read_capture_window_cookie_header(
    app: tauri::AppHandle,
    station_id: &str,
    station_base_url: &str,
) -> Result<String, String> {
    let label = capture_window_label(station_id);
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| "网页登录授权窗口不存在，请重新打开授权窗口。".to_string())?;
    let urls = collectors::url::collector_base_urls(station_base_url);
    let target = tauri::Url::parse(&urls.management_base_url)
        .map_err(|error| format!("站点管理地址无法用于读取 Cookie: {error}"))?;

    let cookies = tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(target))
        .await
        .map_err(|error| format!("读取网页登录授权 Cookie 任务失败: {error}"))?
        .map_err(|error| format!("读取网页登录授权 Cookie 失败: {error}"))?;

    let pairs = cookies
        .into_iter()
        .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
        .collect::<Vec<_>>();
    capture::web_authorization::build_cookie_header_from_pairs(&pairs)
        .ok_or_else(|| "网页登录授权未捕获到可用 Cookie，请确认已在授权窗口完成登录。".to_string())
}
```

Important: keep this as `async`. Tauri documents that reading cookies can deadlock on Windows when done from a synchronous command or event handler.

- [ ] **Step 4: Run cargo check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS. If it fails because `tauri::Url` is unavailable in the locked Tauri version, replace `tauri::Url::parse` with `url::Url::parse` and add `url = "2"` to `src-tauri/Cargo.toml`.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands/mod.rs src-tauri/src/services/capture/web_authorization.rs
git commit -m "feat: read web authorization cookies from capture window"
```

---

### Task 5: Add Verified Finish Command

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Test: existing command compile via `cargo check`

- [ ] **Step 1: Add command skeleton and expect compile failure**

Add this command next to `finish_capture_session` in `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub async fn finish_web_authorization_session(
    app: tauri::AppHandle,
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let cookie_header =
        read_capture_window_cookie_header(app.clone(), &station_id, &station.base_url).await?;
    let urls = collectors::url::collector_base_urls(&station.base_url);
    let verified = capture::web_authorization::verify_newapi_cookie_session(
        &urls.management_base_url,
        &cookie_header,
        std::time::Duration::from_secs(20),
    )?;

    database.persist_station_session_with_data_key(
        crate::models::credentials::PersistStationSessionInput {
            station_id: station_id.clone(),
            access_token: None,
            refresh_token: None,
            cookie: Some(verified.cookie_header),
            newapi_user_id: Some(verified.newapi_user_id),
            token_expires_at: None,
            session_expires_at: None,
            session_source: verified.session_source,
        },
        secrets.data_key(),
    )?;

    finish_capture_session(database, sessions, station_id)
}
```

- [ ] **Step 2: Register the command**

Find the `tauri::generate_handler!` list in `src-tauri/src/lib.rs` or the command registration file. Add:

```rust
finish_web_authorization_session,
```

- [ ] **Step 3: Run cargo check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS after imports and function call signatures are corrected. If `finish_capture_session(database, sessions, station_id)` cannot be called because the command signature consumes `State`, extract the snapshot insertion body into a helper:

```rust
fn finish_capture_session_from_events(
    database: &AppDatabase,
    sessions: &capture::session::CaptureSessionStore,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let events = sessions.take_events(&station_id)?;
    let (summary, normalized, raw) = capture::summarize_events(&events);
    let status = normalized
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("partial")
        .to_string();
    let error_message = if events.is_empty() {
        Some("未捕获到后台接口响应，请确认已在网页登录授权窗口完成登录并打开后台页面。".to_string())
    } else {
        None
    };
    let snapshot = database.insert_collector_snapshot(
        &station_id,
        "webview-capture",
        &status,
        summary,
        normalized,
        Some(raw),
        error_message,
    )?;
    Ok(CollectorRunResult {
        snapshot,
        events: Vec::new(),
    })
}
```

Then make both commands call the helper.

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: finish verified web authorization sessions"
```

---

### Task 6: Prevent Password Fallback for Web Authorization Sessions

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/client.rs`
- Test: `src-tauri/src/services/collectors/adapters/newapi/client.rs` or `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [ ] **Step 1: Add failing test for Web authorization auth expiry**

Add a unit test near existing NewAPI auth context tests:

```rust
#[test]
fn web_authorization_session_auth_failure_requires_manual_reauthorization() {
    let error = super::manual_required_error(
        "Web authorization session expired; please re-authorize in the login window.",
    );

    match error {
        super::NewApiRequestError::ManualRequired { code, message } => {
            assert_eq!(code, "manual_session_required");
            assert!(message.contains("re-authorize"));
        }
        _ => panic!("expected manual required error"),
    }
}
```

This is a characterization test for the error shape before adding resolver logic. It should pass if `manual_required_error` is visible to the test module; if it is private to a nested module, add the test in the same module.

- [ ] **Step 2: Add resolver branch**

In `resolve_auth_context`, after loading `session`, compute:

```rust
let session_source = session.session_source.clone();
```

When session has cookie and user id, keep current behavior. When a later request receives `AuthRequired`, `authenticated_json` currently invalidates the used credential and calls `resolve_auth_context` again. Add a helper:

```rust
fn is_web_authorization_source(source: &str) -> bool {
    source == "web_authorization" || source == "webview_capture"
}
```

In the `Err(NewApiRequestError::AuthRequired { .. }) if attempt == 0` branch, before invalidation, check the original session source. If it is Web authorization, return:

```rust
return Err(manual_required_error(
    "Web authorization session expired; please re-authorize in the login window.",
));
```

- [ ] **Step 3: Preserve password login for password sources**

Add a test or extend an existing one so password-login stations still refresh on auth failure. Use existing tests around `password_login`, `auth_refresh`, or live fixture helpers. The expected summary/source after auth recovery should remain `"auth_refresh"` or `"password_login"` according to the existing test.

- [ ] **Step 4: Run NewAPI tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml newapi -- --nocapture
```

Expected: PASS. Existing ignored live tests remain ignored.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/collectors/adapters/newapi/client.rs src-tauri/src/services/collectors/adapters/newapi/mod.rs
git commit -m "fix: require reauthorization for expired web sessions"
```

---

### Task 7: Frontend API and UI Wording

**Files:**
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Test: `scripts/web-authorization-session-source.test.mjs`

- [ ] **Step 1: Add source guard test**

Create `scripts/web-authorization-session-source.test.mjs`:

```javascript
import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const collectorsPage = readFileSync("src/features/collectors/CollectorsPage.tsx", "utf8");
const collectorApi = readFileSync("src/lib/api/collector.ts", "utf8");

assert.match(collectorApi, /finishWebAuthorizationSession/);
assert.match(collectorsPage, /网页登录授权/);
assert.doesNotMatch(collectorsPage, /网页登录捕获（实验）/);
assert.doesNotMatch(collectorsPage, /实验性网页登录捕获已打开/);

console.log("web authorization session UI source guard passed");
```

- [ ] **Step 2: Run RED**

Run:

```powershell
node .\scripts\web-authorization-session-source.test.mjs
```

Expected: FAIL because the new API wrapper and wording do not exist yet.

- [ ] **Step 3: Add API wrapper**

In `src/lib/api/collector.ts`, add:

```ts
export function finishWebAuthorizationSession(stationId: string) {
  return invoke<CollectorRunResult>("finish_web_authorization_session", { stationId }).catch((error) => {
    if (isTauriUnavailable(error)) {
      return createMemoryRun(stationId, "webview-capture", "manual_required");
    }
    throw error;
  });
}
```

Keep `finishCaptureSession` for raw capture snapshots.

- [ ] **Step 4: Update UI action names**

In `src/features/collectors/CollectorsPage.tsx`:

Replace user-visible strings:

```tsx
"网页登录捕获（实验）"
```

with:

```tsx
"网页登录授权"
```

Replace toast success:

```tsx
toast.success("实验性网页登录捕获已打开");
```

with:

```tsx
toast.success("网页登录授权窗口已打开");
```

Replace finish handler call:

```ts
const result = await finishCaptureSession(selectedStation.id);
```

with:

```ts
const result = await finishWebAuthorizationSession(selectedStation.id);
```

Import `finishWebAuthorizationSession` from `@/lib/api/collector`.

- [ ] **Step 5: Run source guard**

Run:

```powershell
node .\scripts\web-authorization-session-source.test.mjs
```

Expected: PASS.

- [ ] **Step 6: Run frontend check**

Run:

```powershell
pnpm.cmd build
```

Expected: PASS. If this repo's build is too broad for the task and fails on unrelated existing issues, run the existing TypeScript/Vite check command from `package.json` and record the exact failure.

- [ ] **Step 7: Commit**

```powershell
git add src/lib/api/collector.ts src/lib/types/collector.ts src/features/collectors/CollectorsPage.tsx scripts/web-authorization-session-source.test.mjs
git commit -m "feat: surface web authorization sessions"
```

---

### Task 8: Add KamiAPI Preset as a Consumer of Generic Flow

**Files:**
- Modify: `src/features/stations/providerPresets.ts`
- Test: existing preset/source tests if present; otherwise add `scripts/kamiapi-provider-preset.test.mjs`

- [ ] **Step 1: Add failing preset test**

Create `scripts/kamiapi-provider-preset.test.mjs`:

```javascript
import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const source = readFileSync("src/features/stations/providerPresets.ts", "utf8");

assert.match(source, /kamiapi/);
assert.match(source, /卡米API/);
assert.match(source, /https:\/\/www\.kamiapi\.top/);
assert.match(source, /stationType:\s*"newapi"/);

console.log("kamiapi provider preset source guard passed");
```

- [ ] **Step 2: Run RED**

Run:

```powershell
node .\scripts\kamiapi-provider-preset.test.mjs
```

Expected: FAIL because the preset does not exist.

- [ ] **Step 3: Add preset id**

In `src/features/stations/providerPresets.ts`, extend `ProviderPresetId`:

```ts
  | "kamiapi"
```

- [ ] **Step 4: Add preset**

Add this item to the preset list:

```ts
{
  id: "kamiapi",
  name: "卡米API",
  description: "NewAPI 魔改站，推荐使用网页登录授权完成会话采集。",
  stationType: "newapi",
  baseUrl: "https://www.kamiapi.top",
}
```

- [ ] **Step 5: Run preset test**

Run:

```powershell
node .\scripts\kamiapi-provider-preset.test.mjs
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/features/stations/providerPresets.ts scripts/kamiapi-provider-preset.test.mjs
git commit -m "feat: add kamiapi newapi preset"
```

---

### Task 9: Add Authorization Diagnostics to Snapshots

**Files:**
- Modify: `src-tauri/src/services/capture/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Test: `src-tauri/src/services/capture/mod.rs`

- [ ] **Step 1: Add failing snapshot summary test**

In `src-tauri/src/services/capture/mod.rs`, add a pure helper and test target:

```rust
#[test]
fn web_authorization_summary_reports_session_source_without_cookie_value() {
    let summary = super::web_authorization_summary("success", Some("web_authorization"), true);
    let serialized = summary.to_string();

    assert!(serialized.contains("web_authorization"));
    assert!(serialized.contains("cookiePresent"));
    assert!(!serialized.contains("session=abc"));
}
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization_summary_reports_session_source_without_cookie_value -- --nocapture
```

Expected: FAIL because `web_authorization_summary` does not exist.

- [ ] **Step 3: Implement summary helper**

Add in `capture/mod.rs`:

```rust
pub(crate) fn web_authorization_summary(
    status: &str,
    session_source: Option<&str>,
    cookie_present: bool,
) -> Value {
    json!({
        "mode": "web_authorization",
        "status": status,
        "sessionSource": session_source.unwrap_or("none"),
        "cookiePresent": cookie_present,
    })
}
```

- [ ] **Step 4: Merge summary into finish command**

In `finish_web_authorization_session`, after verifying and before inserting the final snapshot, ensure the snapshot summary includes:

```rust
"mode": "web_authorization"
"sessionSource": "web_authorization"
"cookiePresent": true
```

Do this by adding the Web authorization summary to the existing capture summary object after `capture::summarize_events(&events)`, not by storing raw cookie text.

- [ ] **Step 5: Run capture tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization_summary -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml capture_ -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/services/capture/mod.rs src-tauri/src/commands/mod.rs
git commit -m "feat: add web authorization diagnostics"
```

---

### Task 10: End-to-End Local Fixture Test

**Files:**
- Create: `scripts/web-authorization-fixture.test.mjs` or Rust integration-style unit in `src-tauri/src/commands/mod.rs`
- Modify: implementation files only if the fixture exposes a real bug

- [ ] **Step 1: Add local HTTP fixture scenario**

Prefer Rust if command internals can be tested without constructing a full Tauri app. If not, add `scripts/web-authorization-fixture.test.mjs` as a source-level guard plus rely on Rust unit tests for behavior. The fixture must cover:

```text
1. /login returns HTML.
2. /api/user/self returns 401 without Cookie.
3. /api/user/self returns {"success":true,"data":{"id":42}} with Cookie: session=abc.
4. verifier persists only cookie presence and user id in normalized output.
```

- [ ] **Step 2: Run all focused backend tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml capture_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml newapi -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Run frontend/source tests**

Run:

```powershell
node .\scripts\web-authorization-session-source.test.mjs
node .\scripts\kamiapi-provider-preset.test.mjs
```

Expected: PASS.

- [ ] **Step 4: Run full validation**

Run:

```powershell
pnpm.cmd build
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS. Record any unrelated pre-existing failures with exact command output and do not mask them as success.

- [ ] **Step 5: Manual runtime QA**

Run the app through the fresh-source path:

```powershell
pnpm.cmd tauri:dev
```

Manual checks:
- Add or edit a NewAPI station with base URL `https://www.kamiapi.top`.
- Open collectors page.
- Click `网页登录授权`.
- Complete login in the capture window.
- Open console or user page in the capture window so `/api/user/self` is requested.
- Click finish.
- Confirm station credentials show a stored login/session state.
- Run balance/groups/models collection.
- Confirm cookies or API keys are never printed in visible logs or snapshots.

- [ ] **Step 6: Commit final verification fixture**

```powershell
git add scripts/web-authorization-fixture.test.mjs
git commit -m "test: cover web authorization fixture flow"
```

If no JS fixture was needed because Rust tests cover the flow, skip this commit and include the focused Rust evidence in the closeout.

---

## Acceptance Criteria

- Web authorization is a generic flow, not a KamiAPI-only special case.
- KamiAPI can be added as a NewAPI preset that uses the generic flow.
- The app can persist a verified `cookie + newapi_user_id` session after a WebView login.
- HttpOnly cookies are read from Tauri WebView cookie storage, not from injected JavaScript.
- Expired Web authorization sessions return a clear manual reauthorization error and do not fall back to password login.
- Capture snapshots include diagnostic metadata but never raw cookies, tokens, or API keys.
- Existing Sub2API auth retry behavior remains unchanged.
- Existing NewAPI password login behavior remains available for stations that support it.
- Focused Rust tests, source guard scripts, `cargo check`, and frontend build/type checks pass or report exact unrelated blockers.

## Final Verification Commands

Run:

```powershell
node .\scripts\web-authorization-session-source.test.mjs
node .\scripts\kamiapi-provider-preset.test.mjs
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml capture_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml newapi -- --nocapture
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd build
```

Manual runtime:

```powershell
pnpm.cmd tauri:dev
```

Verify the KamiAPI Web authorization flow with a real login only when the user is ready to interactively complete the login.

## Plan Self-Review

- Spec coverage: The plan covers generic Web authorization, KamiAPI as a preset consumer, HttpOnly cookie capture, NewAPI session persistence, auth-expiry behavior, frontend UI, diagnostics, and verification.
- Placeholder scan: No deferred placeholders are present; each task has concrete files, code snippets, commands, and expected results.
- Type consistency: The stable new backend source is `web_authorization`; the new frontend wrapper is `finishWebAuthorizationSession`; the new Tauri command is `finish_web_authorization_session`; the session struct is `VerifiedWebAuthorizationSession`.
