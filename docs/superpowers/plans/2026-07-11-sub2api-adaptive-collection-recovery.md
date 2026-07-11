# Sub2API Adaptive Collection Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Sub2API balance and group collection recover from rejected login tokens and transient upstream failures without false failure events or unbounded station blocking.

**Architecture:** Add a collector-local request recovery engine that owns classification, bounded retries, task budget, and additive attempt diagnostics. Keep Sub2API adapters responsible for endpoint order and fact parsing, while extending the existing login transport with a budget-aware recovery entry point. Multi-key balance requests run in fair rounds so every key receives an initial attempt before retries.

**Tech Stack:** Rust, Tauri 2 backend services, `ureq`, `serde_json`, SQLite-backed collector snapshots, Node source-contract scripts

---

## File Structure

- Create `src-tauri/src/services/collectors/adapters/request_recovery.rs`: request policy, task budget, failure classification, retry execution, recovery actions, and redacted JSON diagnostics.
- Modify `src-tauri/src/services/collectors/adapters/mod.rs`: expose the recovery module to adapters.
- Modify `src-tauri/src/services/collectors/adapters/sub2api.rs`: replace local endpoint retry code, share refreshed login state, implement fair balance rounds, and map final recovery failures.
- Modify `src-tauri/src/services/collectors/sub2api.rs`: add a budget-aware login token function without changing detection or manual credential-test behavior.
- Modify `scripts/station-auto-collector.test.mjs`: assert the adaptive recovery module remains connected to scheduled Sub2API collection and bounded by a shared task budget.

### Task 1: Request Recovery Contract

**Files:**
- Create: `src-tauri/src/services/collectors/adapters/request_recovery.rs`
- Modify: `src-tauri/src/services/collectors/adapters/mod.rs`

- [ ] **Step 1: Write failing unit tests for classification and retry limits**

Add tests in `request_recovery.rs` for:

```rust
#[test]
fn classifies_retryable_and_permanent_results() {
    assert_eq!(classify_result(None, false, false), Some(FailureKind::NetworkTimeout));
    assert_eq!(classify_result(Some(429), false, false), Some(FailureKind::RateLimited));
    assert_eq!(classify_result(Some(502), false, false), Some(FailureKind::Upstream5xx));
    assert_eq!(classify_result(Some(422), false, false), Some(FailureKind::PermanentHttp));
    assert_eq!(classify_result(Some(200), true, false), Some(FailureKind::InvalidJson));
    assert_eq!(classify_result(Some(200), false, true), None);
}

#[test]
fn transient_execution_stops_after_three_attempts() {
    let policy = RequestPolicy::for_tests(Duration::from_secs(1));
    let budget = CollectionAttemptBudget::new(policy.task_budget);
    let mut calls = 0;
    let execution = execute_json_request(&policy, &budget, None, |_, _| {
        calls += 1;
        EndpointJsonResult::http("/test", 502, None, Duration::from_millis(1))
    }, |_| unreachable!());
    assert_eq!(calls, 3);
    assert_eq!(execution.failure_kind, Some(FailureKind::Upstream5xx));
}
```

Also cover `400/404/422` single-attempt behavior, malformed JSON retry once, `429` delay exceeding remaining budget, and no request after budget exhaustion.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_recovery -- --nocapture
```

Expected: compilation fails because `request_recovery` types and functions do not exist.

- [ ] **Step 3: Implement the recovery primitives**

Implement these concrete types:

```rust
pub(crate) enum FailureKind {
    AuthRejected,
    AuthRefreshFailed,
    NetworkTimeout,
    RateLimited,
    Upstream5xx,
    InvalidJson,
    PermanentHttp,
    TaskBudgetExhausted,
}

pub(crate) enum RecoveryAction {
    AuthRefresh,
    TransientRetry,
}

pub(crate) struct RequestPolicy {
    pub max_attempts: usize,
    pub malformed_json_max_attempts: usize,
    pub task_budget: Duration,
    pub retry_delays: [Duration; 2],
}

pub(crate) struct CollectionAttemptBudget {
    started_at: Instant,
    limit: Duration,
}

pub(crate) struct AttemptRecord {
    pub attempt: usize,
    pub status: Option<u16>,
    pub duration_ms: i64,
    pub failure_kind: Option<FailureKind>,
    pub action: &'static str,
}

pub(crate) struct EndpointJsonResult {
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub duration_ms: i64,
    pub payload: Option<Value>,
    pub error_message: Option<String>,
    pub retry_after: Option<Duration>,
}

pub(crate) struct RequestExecution {
    pub result: EndpointJsonResult,
    pub attempts: Vec<AttemptRecord>,
    pub failure_kind: Option<FailureKind>,
    pub recovery_actions: Vec<RecoveryAction>,
}
```

`RequestExecution::to_redacted_json()` must preserve top-level `url`, `status`, `ok`, and `durationMs`; add `path`, `attemptCount`, `failureKind`, `recoveryActions`, and nested `attempts`. Top-level duration is total executor wall time. Never serialize credentials or response bodies.

`execute_json_request` accepts the current credential, an optional auth refresh closure, and a request closure receiving `(credential, remaining_timeout)`. Auth refresh counts as one recovery action; endpoint requests never exceed three total attempts; malformed JSON never exceeds two total attempts. A refreshed credential rejected again stops without another refresh.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run the Task 1 command again.

Expected: all `request_recovery` tests pass.

- [ ] **Step 5: Commit the recovery contract**

```powershell
git add -- src-tauri/src/services/collectors/adapters/request_recovery.rs src-tauri/src/services/collectors/adapters/mod.rs
git commit -m "feat: add bounded collector request recovery"
```

### Task 2: Budget-Aware Login Recovery

**Files:**
- Modify: `src-tauri/src/services/collectors/sub2api.rs`

- [ ] **Step 1: Add failing login budget tests**

Add focused tests using a local login server:

```rust
#[test]
fn login_recovery_stops_when_budget_is_exhausted() {
    let server = SlowLoginServer::start();
    let outcome = login_access_token_with_budget(
        &server.base_url,
        "user@example.test",
        "secret",
        Duration::from_millis(30),
    );
    assert!(matches!(outcome, Err(error) if error.contains("task_budget_exhausted")));
    assert_eq!(server.request_count(), 1);
}

#[test]
fn login_recovery_does_not_restart_candidate_sequence() {
    let server = FlakyLoginServer::start();
    let outcome = login_access_token_with_budget(
        &server.base_url,
        "user@example.test",
        "secret",
        Duration::from_secs(1),
    ).expect("login outcome");
    assert_eq!(outcome.access_token.as_deref(), Some("fresh-token"));
    assert!(server.request_count() <= 2);
}
```

- [ ] **Step 2: Run focused login tests and verify RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml login_recovery_ -- --nocapture
```

Expected: compilation fails because `login_access_token_with_budget` is absent.

- [ ] **Step 3: Implement the budget-aware login entry point**

Keep `test_login_credentials` unchanged. Add:

```rust
pub(crate) fn login_access_token_with_budget(
    base_url: &str,
    username: &str,
    password: &str,
    budget: Duration,
) -> Result<LoginTokenOutcome, String>
```

Refactor the internal login candidate loop to accept an `Instant` deadline. Before each candidate request, derive connect/read/write timeouts from the remaining duration and stop with the stable redacted marker `task_budget_exhausted` when no time remains. A network error or `5xx` can retry the same candidate once. A second transient failure stops the recovery sequence. `429` respects `Retry-After` only if it fits the deadline. Permanent candidate responses may continue through the existing three paths and three username fields while time remains.

- [ ] **Step 4: Run login and existing Sub2API credential tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml login_recovery_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_login -- --nocapture
```

Expected: new and existing tests pass.

- [ ] **Step 5: Commit login budgeting**

```powershell
git add -- src-tauri/src/services/collectors/sub2api.rs
git commit -m "feat: bound Sub2API login recovery"
```

### Task 3: Group Collection Integration

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`

- [ ] **Step 1: Extend group tests for shared auth and combined recovery**

Add tests named:

```rust
sub2api_groups_share_refreshed_token_across_endpoints
sub2api_groups_record_auth_then_transient_recovery
sub2api_groups_reject_refreshed_token_without_login_loop
sub2api_groups_preserve_top_level_endpoint_counts
```

The combined fixture must return `401`, then accept login, then return `502`, then `200`. Assert:

```rust
assert_eq!(output.status, "success");
assert_eq!(summary["endpointResults"][0]["ok"], true);
assert_eq!(summary["endpointResults"][0]["recoveryActions"], json!([
    "auth_refresh",
    "transient_retry"
]));
assert_eq!(summary["endpointResults"][0]["attemptCount"], 3);
```

- [ ] **Step 2: Run group tests and verify meaningful RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_groups_ -- --nocapture
```

Expected: new assertions fail because current group output has no shared recovery execution or additive attempt diagnostics.

- [ ] **Step 3: Replace local group retry logic**

Remove the adapter-local `retry_transient_fetch_json_with_bearer`. Create one `CollectionAttemptBudget::production()` and one task-local login state for both group endpoints. Route both endpoints through `execute_json_request`, pass remaining budget into `login_access_token_with_budget`, persist at most one replacement token, and pass the updated token to the second endpoint.

Keep current group parsing and status behavior:

```rust
let status = match success_count {
    2 => "success",
    1 if !facts.groups.is_empty() => "partial",
    _ => "failed",
};
```

Final `error_code` comes from the explicit no-facts precedence in the design. Endpoint results remain one top-level record per logical endpoint.

- [ ] **Step 4: Run group tests and verify GREEN**

Run the Task 3 command again.

Expected: all group tests pass, including the pre-existing one-shot `502` regression.

- [ ] **Step 5: Commit group integration**

```powershell
git add -- src-tauri/src/services/collectors/adapters/sub2api.rs
git commit -m "feat: apply adaptive recovery to Sub2API groups"
```

### Task 4: Balance Recovery and Multi-Key Fairness

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`

- [ ] **Step 1: Add failing balance recovery tests**

Add fixtures and tests named:

```rust
sub2api_balance_refreshes_rejected_account_token
sub2api_balance_does_not_emit_failed_snapshot_after_internal_recovery
sub2api_balance_attempts_all_keys_before_transient_retries
sub2api_balance_stops_at_task_budget
sub2api_balance_does_not_retry_permanent_http_errors
sub2api_balance_diagnostics_redact_credentials
```

The stale-token regression must seed a stale session plus saved credentials, return `401` from `/user/profile`, return a fresh token from login, then return a usable balance. Assert final success, persisted fresh token, `recoveryActions == ["auth_refresh"]`, and no `collector_failed` change event after applying through `collect_station_task`.

The fairness fixture must expose two keys. Key A returns `502` on its first request; Key B returns `200`. Assert the server order is `A, B, A`, not `A, A, B`.

- [ ] **Step 2: Run balance tests and verify RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_balance_ -- --nocapture
```

Expected: stale-token recovery and fair-round assertions fail against current sequential single-attempt behavior.

- [ ] **Step 3: Implement fair key rounds and account recovery**

Create one balance task budget. Decrypt enabled keys first, recording secret failures without exposing values. Execute one `/v1/usage` request per usable key in round one. Queue only transient failures for rounds two and three; never retry key `401/403` or permanent `4xx`.

If no balance facts exist, call account fallback with the same task budget. `/user/profile` and `/auth/me` share a mutable login token and the one allowed authentication recovery sequence. A successful profile response without a balance field proceeds to `/auth/me` without repeating profile. If any key or account balance fact exists, preserve overall `success`; sibling failures remain diagnostic.

Map no-facts outcomes in this order:

```text
task_budget_exhausted
auth_rejected after refresh
auth_refresh_failed with redacted cause
manual_required / credential_unavailable
exhausted endpoint failure kind
no_balance_facts fallback
```

- [ ] **Step 4: Run balance, event, and redaction tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_balance_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml collector_recovery_event_records_the_recovered_task_type -- --nocapture
```

Expected: all focused tests pass and internal recovery produces no false failure event.

- [ ] **Step 5: Commit balance integration**

```powershell
git add -- src-tauri/src/services/collectors/adapters/sub2api.rs
git commit -m "feat: recover Sub2API balance collection"
```

### Task 5: Scheduled Collector Contract and Full Verification

**Files:**
- Modify: `scripts/station-auto-collector.test.mjs`
- Verify: all files from Tasks 1-4

- [ ] **Step 1: Add failing source-contract assertions**

Assert that:

```javascript
assert.ok(
  sub2apiAdapterSource.includes("CollectionAttemptBudget") &&
    sub2apiAdapterSource.includes("recoveryActions"),
  "Sub2API scheduled collection should use bounded adaptive recovery diagnostics",
);

assert.ok(
  sub2apiLoginSource.includes("login_access_token_with_budget"),
  "Sub2API auth recovery should share the collection task budget",
);
```

Keep all existing scheduler timeout assertions.

- [ ] **Step 2: Run the source-contract test and verify RED, then GREEN**

Run before and after adding the assertions/wiring:

```powershell
node scripts\station-auto-collector.test.mjs
```

Expected before completed wiring: assertion failure. Expected after wiring: PASS.

- [ ] **Step 3: Run the complete focused Rust suite**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_recovery -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml login_recovery_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_groups_ -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_balance_ -- --nocapture
```

Expected: all pass.

- [ ] **Step 4: Run backend and repository checks**

```powershell
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
node scripts\station-auto-collector.test.mjs
git diff --check
```

Expected: exit code 0 for every command. Existing unrelated working-tree changes are excluded from staging and are not repaired or reformatted.

- [ ] **Step 5: Audit secrets and scope**

Run:

```powershell
rg -n "Authorization|access_token|password|cookie" src-tauri/src/services/collectors/adapters/request_recovery.rs
git diff --name-only HEAD~4..HEAD
```

Expected: the recovery module contains no serialization of credential values; changed production files stay within the five paths declared by this plan.

- [ ] **Step 6: Commit the scheduled contract**

```powershell
git add -- scripts/station-auto-collector.test.mjs
git commit -m "test: cover adaptive station collection recovery"
```

## Final Acceptance

- A rejected balance login token refreshes inside the balance task rather than waiting for group collection.
- Network, `408`, `429`, `5xx`, and malformed JSON use bounded error-specific recovery.
- Permanent `4xx` and rejected API keys are not retried blindly.
- Multiple keys receive a fair initial attempt.
- Login candidates and endpoint attempts share the 30-second task budget.
- Top-level endpoint `ok` remains compatible with collector run counts.
- Internal recovery records additive diagnostics without false failure/recovery events.
- No credentials or response bodies appear in persisted attempt diagnostics.
