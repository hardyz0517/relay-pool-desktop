# Automatic Web Authorization Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make NewAPI/KamiAPI popup authorization complete automatically after the login WebView observes a verified identity endpoint, without requiring the user to press a second finish button in the normal flow.

**Architecture:** Reuse the existing capture window, `record_capture_event`, and `finish_web_authorization_session` command. Add provider-family candidate recognition for NewAPI identity responses in Rust, expose it through sanitized capture status, and update the injected capture script to call the same native finish command through an idempotent page-side guard after `record_capture_event` accepts a candidate. The stations page then treats a start-authorization action as a pending automatic flow and listens for query/state changes instead of requiring manual completion.

**Tech Stack:** Tauri 2, Rust capture services and commands, React/TypeScript station list UI, source regression scripts, focused Cargo tests.

---

## File Structure

Modify:
- `src-tauri/src/services/capture/web_authorization.rs` - add NewAPI completion candidate recognition and tests.
- `src-tauri/src/commands/mod.rs` - make `record_capture_event` return candidate status and inject automatic finish trigger into capture-window JavaScript.
- `src-tauri/src/services/capture/session.rs` - extend status only if needed by the candidate result.
- `src-tauri/src/models/capture.rs` - add sanitized candidate metadata if the existing status type is too small.
- `src/features/stations/StationsPage.tsx` - update authorization start flow to rely on automatic completion and refresh once after success.
- `src/lib/api/collector.ts` - keep manual finish wrapper as fallback; no new frontend command if injected script can call native directly.
- `scripts/station-list-risk-tags.test.mjs` - strengthen the source guard for NewAPI row authorization.

Create:
- `scripts/automatic-web-authorization-completion.test.mjs` - source guard that proves the capture script triggers `finish_web_authorization_session` only after candidate recognition and keeps the manual finish API as fallback.

Test:
- `src-tauri/src/services/capture/web_authorization.rs` unit tests.
- `src-tauri/src/commands/mod.rs` unit tests for generated script content.
- Existing `scripts/station-list-risk-tags.test.mjs`.
- New `scripts/automatic-web-authorization-completion.test.mjs`.

---

### Task 1: Add NewAPI Completion Candidate Recognition

**Files:**
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Test: `src-tauri/src/services/capture/web_authorization.rs`

- [ ] **Step 1: Write failing candidate tests**

Add tests:

```rust
#[test]
fn recognizes_successful_newapi_self_candidate() {
    let payload = json!({ "success": true, "data": { "id": 42 } });

    assert!(is_newapi_completion_candidate(
        "/api/user/self",
        Some(200),
        Some(&payload),
    ));
}

#[test]
fn rejects_unauthenticated_or_unrelated_completion_candidates() {
    let payload = json!({ "success": true, "data": { "id": 42 } });

    assert!(!is_newapi_completion_candidate("/api/user/self", Some(401), Some(&payload)));
    assert!(!is_newapi_completion_candidate("/api/token", Some(200), Some(&payload)));
    assert!(!is_newapi_completion_candidate("/api/user/self", Some(200), Some(&json!({ "success": true }))));
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml newapi_self_candidate -- --nocapture
```

Expected: FAIL because `is_newapi_completion_candidate` does not exist.

- [ ] **Step 3: Implement minimal recognition**

Add:

```rust
pub(crate) fn is_newapi_completion_candidate(
    request_path: &str,
    status: Option<i64>,
    response_json: Option<&Value>,
) -> bool {
    matches!(status, Some(200..=299))
        && request_path
            .split('?')
            .next()
            .unwrap_or(request_path)
            .trim_end_matches('/')
            .eq_ignore_ascii_case("/api/user/self")
        && response_json.and_then(extract_verified_user_id).is_some()
}
```

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml newapi_self_candidate -- --nocapture
```

Expected: PASS.

---

### Task 2: Surface Sanitized Candidate Status From `record_capture_event`

**Files:**
- Modify: `src-tauri/src/models/capture.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Test: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing command/status test**

Add a pure helper test in `commands::tests`:

```rust
#[test]
fn captured_newapi_self_event_marks_web_authorization_candidate() {
    let input = CapturedHttpEventInput {
        station_id: "station-1".to_string(),
        source_window_id: "capture-station-1".to_string(),
        page_url: "https://relay.example/console".to_string(),
        request_url: "https://relay.example/api/user/self".to_string(),
        request_path: Some("/api/user/self".to_string()),
        method: "GET".to_string(),
        status: Some(200),
        content_type: Some("application/json".to_string()),
        started_at: None,
        finished_at: None,
        duration_ms: None,
        response_kind: Some("json".to_string()),
        response_size: None,
        response_json: Some(json!({ "success": true, "data": { "id": 42 } })),
        response_text: None,
        error_message: None,
    };

    assert!(web_authorization_candidate_from_input(&input));
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml captured_newapi_self_event_marks_web_authorization_candidate -- --nocapture
```

Expected: FAIL because helper/status does not exist.

- [ ] **Step 3: Add helper and status field**

Add to `CaptureSessionStatus` if absent:

```rust
pub web_authorization_candidate: bool,
```

Add helper in `commands/mod.rs`:

```rust
fn web_authorization_candidate_from_input(input: &CapturedHttpEventInput) -> bool {
    capture::web_authorization::is_newapi_completion_candidate(
        input.request_path
            .as_deref()
            .unwrap_or_else(|| path_from_request_url(&input.request_url)),
        input.status,
        input.response_json.as_ref(),
    )
}
```

If borrowing a temporary is awkward, compute an owned fallback path before calling the helper.

- [ ] **Step 4: Set status after push**

In `record_capture_event`, compute the candidate before moving `input`, push the event, then set the sanitized candidate flag on the returned status. Do not expose cookies, response bodies, or user ids.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml captured_newapi_self_event_marks_web_authorization_candidate -- --nocapture
```

Expected: PASS.

---

### Task 3: Trigger Native Finish Automatically From Capture Script

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Test: `src-tauri/src/commands/mod.rs`
- Test: `scripts/automatic-web-authorization-completion.test.mjs`

- [ ] **Step 1: Add failing generated-script test**

Add:

```rust
#[test]
fn capture_script_invokes_web_authorization_finish_after_candidate() {
    let script = capture_script("station-1", "capture-station-1", None, None);

    assert!(script.contains("finish_web_authorization_session"));
    assert!(script.contains("webAuthorizationCandidate"));
    assert!(script.contains("__relayPoolAuthorizationFinishInFlight"));
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml capture_script_invokes_web_authorization_finish_after_candidate -- --nocapture
```

Expected: FAIL because the capture script does not call the finish command.

- [ ] **Step 3: Implement page-side idempotent trigger**

In `capture_script`, change `send` to await `record_capture_event` and call finish only when the sanitized status says it is a candidate:

```javascript
  const tryFinishWebAuthorization = (status) => {
    if (!invoke || !status || !status.webAuthorizationCandidate) return;
    if (window.__relayPoolAuthorizationFinishInFlight) return;
    window.__relayPoolAuthorizationFinishInFlight = true;
    invoke("finish_web_authorization_session", { stationId })
      .finally(() => {
        window.__relayPoolAuthorizationFinishInFlight = false;
      })
      .catch(() => undefined);
  };
  const send = (input) => {
    if (!invoke) return;
    invoke("record_capture_event", { input })
      .then(tryFinishWebAuthorization)
      .catch(() => undefined);
  };
```

- [ ] **Step 4: Add source guard**

Create `scripts/automatic-web-authorization-completion.test.mjs`:

```javascript
import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const commands = readFileSync("src-tauri/src/commands/mod.rs", "utf8");
const collectorApi = readFileSync("src/lib/api/collector.ts", "utf8");

assert.match(commands, /finish_web_authorization_session/);
assert.match(commands, /webAuthorizationCandidate/);
assert.match(commands, /__relayPoolAuthorizationFinishInFlight/);
assert.match(commands, /record_capture_event/);
assert.match(collectorApi, /finishWebAuthorizationSession/);

console.log("automatic web authorization completion source guard passed");
```

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml capture_script_invokes_web_authorization_finish_after_candidate -- --nocapture
node .\scripts\automatic-web-authorization-completion.test.mjs
```

Expected: PASS.

---

### Task 4: Keep Station Row Authorization Flow Automatic

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `scripts/station-list-risk-tags.test.mjs`
- Test: `scripts/station-list-risk-tags.test.mjs`

- [ ] **Step 1: Add failing source guard**

Extend the station-list assertion so it requires `startManualAuthorization(station.id)` but does not require `finishWebAuthorizationSession` in the row flow:

```javascript
assert.ok(
  pageSource.includes("startManualAuthorization(station.id)") &&
    !pageSource.includes("finishWebAuthorizationSession(station.id)"),
  "station row authorization should start the automatic popup flow without a second required finish action",
);
```

- [ ] **Step 2: Verify RED or existing GREEN**

Run:

```powershell
node .\scripts\station-list-risk-tags.test.mjs
```

Expected: PASS if current row flow already only starts the popup. If it fails, adjust only the row flow.

- [ ] **Step 3: Improve user-visible start toast**

Change the station row success toast to communicate automatic completion:

```ts
toast.success("授权窗口已打开，登录成功后会自动保存会话");
```

Keep the collectors page manual finish button as fallback.

- [ ] **Step 4: Verify source guard**

Run:

```powershell
node .\scripts\station-list-risk-tags.test.mjs
```

Expected: PASS.

---

### Task 5: Verification and Scoped Commit

**Files:**
- Stage only the files changed in Tasks 1-4.

- [ ] **Step 1: Run focused tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml web_authorization -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml capture_script_invokes_web_authorization_finish_after_candidate -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml captured_newapi_self_event_marks_web_authorization_candidate -- --nocapture
node .\scripts\automatic-web-authorization-completion.test.mjs
node .\scripts\station-list-risk-tags.test.mjs
```

Expected: PASS.

- [ ] **Step 2: Run broader checks**

Run:

```powershell
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd build
```

Expected: PASS, or record exact unrelated blockers.

- [ ] **Step 3: Stage scoped files**

Run exact-path staging only:

```powershell
git add docs/superpowers/plans/2026-07-13-automatic-web-authorization-completion.md
git add src-tauri/src/services/capture/web_authorization.rs
git add src-tauri/src/models/capture.rs
git add src-tauri/src/services/capture/session.rs
git add src-tauri/src/commands/mod.rs
git add src/features/stations/StationsPage.tsx
git add scripts/automatic-web-authorization-completion.test.mjs
git add scripts/station-list-risk-tags.test.mjs
```

- [ ] **Step 4: Commit**

Run:

```powershell
git commit -m "feat: auto-complete web authorization sessions"
```

---

## Acceptance Criteria

- KamiAPI/NewAPI popup login can complete without a second main-window finish action.
- `record_capture_event` exposes only a sanitized candidate flag, never cookies, response bodies, or user ids.
- The injected capture script calls `finish_web_authorization_session` only after a successful NewAPI identity candidate.
- Duplicate identity requests are deduplicated by a page-side in-flight guard; native finish remains safe to call repeatedly through the existing verified path.
- Manual `finishWebAuthorizationSession` remains available from the collectors page as fallback.
- Station rows support both Sub2API and NewAPI authorization entry points.
- Focused Rust tests, source guards, `cargo fmt --check`, `cargo check`, and `pnpm.cmd build` are run before claiming completion.

## Plan Self-Review

- Spec coverage: The plan covers automatic completion, native verification as source of truth, sanitized status, duplicate trigger guarding, fallback manual completion, and station-row UX.
- Placeholder scan: No `TBD`, `TODO`, or vague "add tests" placeholders remain.
- Type consistency: The candidate flag is consistently named `web_authorization_candidate` in Rust and serialized as `webAuthorizationCandidate` for JavaScript.
