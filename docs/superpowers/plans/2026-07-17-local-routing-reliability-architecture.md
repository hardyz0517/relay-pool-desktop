# Local Routing Reliability Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Relay Pool's hand-written local HTTP proxy with a bounded Tokio/Hyper/Axum server and one unified execution pipeline while preserving Station Key routing, pricing, balance, health, affinity, logging, and Tauri command contracts.

**Architecture:** Keep Relay Pool's existing Router/Scheduler as the domain authority. Introduce a bounded async ingress, a sealed endpoint-adapter set, a shared Reqwest upstream pool, and a single candidate-attempt state machine; migrate behind an injectable legacy/v2 runtime boundary, prove parity, switch the default, then delete the legacy socket/parser path.

**Tech Stack:** Rust 2021, Tauri 2 async runtime, Tokio 1, Hyper 1, Axum 0.8, Tower 0.5, Tower HTTP 0.6, Reqwest 0.13 with Rustls/stream/json/socks, rusqlite, serde/serde_json, React 18, TypeScript, Node contract tests, Cargo unit and loopback integration tests.

**Approved design:** `docs/superpowers/specs/2026-07-17-local-routing-reliability-upgrade-design.md`

---

## Execution Rules

- Before Task 1, invoke `superpowers:using-git-worktrees`. The isolated worktree must start from the current working-tree state so the existing overlapping edits are preserved. If the environment cannot carry uncommitted edits into a worktree, execute in the current checkout and preserve every pre-existing hunk.
- Never use `git add .` or `git add -A`. Stage only the paths named by the current task. If a named file already contains unrelated edits, stage only the task hunks and prove the staged snapshot with `git diff --cached --name-only` and `git diff --cached`.
- Use RED-GREEN TDD. A task is not complete until its named failing test has been observed failing for the expected reason, the minimal implementation passes it, and the task's regression command passes.
- Keep `Station -> Station Key -> Router/Scheduler` as the only routing domain. Do not introduce a provider plugin registry, Go sidecar, FFI, dynamic translators, async ORM, or new endpoint families.
- Do not claim downstream TCP acknowledgement. The observable completion boundary is Hyper consuming the response body to EOF; early body drop is a downstream disconnect.
- During Tasks 4-17, legacy and v2 may coexist in source but never listen on the same port in one process. Task 18 removes the legacy runtime and selector only after its release precondition passes.

## Target File Map

| Path | Responsibility after completion |
|---|---|
| `src-tauri/src/services/proxy/runtime.rs` | Tauri-facing lifecycle facade and runtime-mode selection only |
| `src-tauri/src/services/proxy/server.rs` | Tokio listener, Hyper HTTP/1.1 connection serving, connection admission, cancellation |
| `src-tauri/src/services/proxy/limits.rs` | Fixed server limits and weighted body-memory budget |
| `src-tauri/src/services/proxy/error.rs` | Stable failure source/code/retry class and OpenAI-compatible error rendering |
| `src-tauri/src/services/proxy/request.rs` | Canonical request, requirements, forwarded-header allowlist, body-budget lease |
| `src-tauri/src/services/proxy/ingress.rs` | Axum routing, CORS, local auth, body collection, request admission |
| `src-tauri/src/services/proxy/execution.rs` | Route plan, retry policy, candidate attempt state machine |
| `src-tauri/src/services/proxy/endpoint_adapter.rs` | Sealed Models/Embeddings/Chat/Responses adapters and Responses-to-Chat bridge |
| `src-tauri/src/services/proxy/upstream.rs` | Shared direct/HTTP/SOCKS Reqwest clients and typed attempt outcomes |
| `src-tauri/src/services/proxy/response_body.rs` | First-chunk bootstrap, stream idle timeout, response-body completion/drop finalization |
| `src-tauri/src/services/proxy/routing_repository.rs` | `spawn_blocking` bridge around current `AppDatabase` route reads and final writes |
| `src-tauri/src/services/proxy/observability.rs` | Existing usage/SSE parsing plus request trace and attempt timing |
| `src-tauri/src/services/proxy/legacy_runtime.rs` | Temporary mechanical home of the old implementation; deleted in Task 18 |
| `src-tauri/src/models/proxy.rs` | Public lifecycle/status/request-log schema |
| `src-tauri/src/services/database.rs` | Existing SQLite implementation; proxy-facing entry points delegated through repository |
| `src/lib/types/proxy.ts` | TypeScript mirror of public status and request-log schema |

### Shared Type Ledger

| Type | Introduced | Final owner |
|---|---:|---|
| `ProxyFailure`, `ProxyFailureCode`, `FailureSource`, `RetryClass` | Task 3 | `error.rs` |
| `ProxyServerLimits`, `BodyBudget`, `BodyBudgetLease`, `RequestLease` | Task 3 | `limits.rs` |
| `CanonicalProxyRequest`, `RequestRequirements`, `ProxyHttpResponse` | Task 3 | `request.rs` |
| `IngressExecutor`, `IngressState` | Task 5 | `ingress.rs` |
| `RunningServer`, `ProxyStartConfig`, `ProxyRuntimeMode` | Task 6 | `server.rs`, `runtime.rs` |
| `RoutingRepository`, `SqliteRoutingRepository`, `FinalRequestOutcome` | Task 7 | `routing_repository.rs` |
| `EndpointAdapter`, `PreparedUpstreamRequest`, `ResponseMode` | Task 8 | `endpoint_adapter.rs` |
| `ProxyRoute`, `UpstreamClientPool`, `UpstreamAttempt`, `ByteStream` | Task 9 | `upstream.rs` |
| `RetryPolicy`, `AttemptExecutor`, `ExecutionEngine`, `PreparedAttempt`, `ProxyExecutionResponse` | Task 10 | `execution.rs` |
| `FinalizationLease`, `FinalizationDispatcher`, `FinalizeOnce`, `FinalizingBody`, `ProxyBodyError` | Task 11 | `response_body.rs` |

## Approved Spec Coverage

| Design requirement | Implementation tasks | Exit evidence |
|---|---|---|
| First-run local key and inbound auth contract | 1, 5 | Fresh import never exports the placeholder; auth/CORS contract tests pass |
| Bounded Hyper/Axum lifecycle, connection/request/body limits | 3, 5, 6 | 64/32 admission tests, weighted body-budget tests, drain/forced-stop tests |
| Existing Router/Scheduler remains domain authority | 7, 10 | Repository and execution tests preserve candidate order, affinity, waits, and revision guards |
| Sealed endpoint translation without plugin sprawl | 8, 11, 15 | Models/Usage/Embeddings/Chat/Responses adapter parity tests |
| One shared Reqwest transport with HTTP/SOCKS support | 3, 9 | Client-pool reuse, proxy mode, redirect, TLS, and dependency-tree checks |
| Typed retry classification and at most three distinct candidates | 10, 14 | Status/idempotency/budget matrix and bootstrap-versus-commit tests |
| Buffered and streaming finalize exactly once | 11, 12, 13, 14 | EOF/error/drop/shutdown tests plus transactional health/log assertions |
| Prepared-stream commit after first non-empty chunk | 14, 15 | First-byte, idle, mid-stream reset, and no-post-commit-failover tests |
| Differential, resource, security, and performance gates | 2, 16 | Legacy/v2 parity, disconnect soak, memory/task bounds, secret scans, p95 gate |
| Default switch, real-client acceptance, rollback window | 17 | CC-Switch/Codex live matrix and debug-only legacy override |
| Legacy parser/socket retirement after a shipped v2 release | 18 | Release precondition, deletion contract, final dependency/source scan |

## Task 0: Protect the Dirty Baseline

**Files:** No writes. Inspect the current checkout and the isolated execution worktree.

- [ ] **Step 1: Record the overlapping dirty paths**

Run:

```powershell
git status --short
git diff -- src-tauri/src/services/database.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/collectors/adapters/newapi/test_support.rs
```

Expected: the three existing modified Rust paths are visible. Save their hunk ownership in the execution notes; do not revert them.

- [ ] **Step 2: Run the current focused baseline**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
pnpm.cmd test:contracts
```

Expected: record the exact pass/fail count. Existing failures become baseline evidence; do not repair unrelated failures inside this migration.

- [ ] **Step 3: Confirm the design commit and worktree base**

Run:

```powershell
git log -1 --oneline -- docs/superpowers/specs/2026-07-17-local-routing-reliability-upgrade-design.md
git diff --name-only HEAD
```

Expected: design commit `fa5bba0` or a descendant is present, and the diff list still contains the preserved user changes. This is a checkpoint; create no commit.

## Task 1: Fix the First-Run Local Key Contract

**Files:**
- Modify: `src-tauri/src/commands/mod.rs:290-347`
- Modify: `src-tauri/src/services/database.rs:714-745`
- Modify: `scripts/local-proxy-auth-contract.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: Write the failing import-contract test**

Extract a pure helper in the command module test target and add this regression before changing the command:

```rust
#[test]
fn ccswitch_import_ensures_placeholder_key_before_building_deeplink() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let status = ProxyStatus {
        running: true,
        lifecycle: ProxyLifecycle::Running,
        bind_addr: "127.0.0.1".to_string(),
        port: 8787,
        started_at: None,
        last_error: None,
        active_requests: 0,
        request_count: 0,
    };

    let (_, deeplink) = prepare_ccswitch_import(&database, &status).expect("import plan");
    let persisted = database.get_local_access_key().expect("persisted key");

    assert_ne!(persisted, "sk-local-pool-change-me");
    assert!(deeplink.contains(&format!("apiKey={}", encode_query_param(&persisted))));
}
```

Use the existing `encode_query_param` helper. Do not add a dependency only for the test.

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml ccswitch_import_ensures_placeholder_key_before_building_deeplink -- --nocapture
```

Expected: FAIL because `prepare_ccswitch_import` does not exist or because the placeholder is still exported.

- [ ] **Step 3: Implement one ensuring path**

Add the helper and route all user-facing reads through the ensuring method:

```rust
fn prepare_ccswitch_import(
    database: &AppDatabase,
    status: &ProxyStatus,
) -> Result<(CcswitchImportResult, String), String> {
    let local_access_key = database.ensure_secure_local_access_key()?;
    let endpoint = format!("http://{}:{}/v1", status.bind_addr, status.port);
    let homepage = format!("http://{}:{}", status.bind_addr, status.port);
    let provider_name = "Relay Pool Desktop".to_string();
    let deeplink = build_ccswitch_provider_deeplink(
        "codex", &provider_name, &homepage, &endpoint, &local_access_key,
    );
    Ok((CcswitchImportResult {
        app: "codex".to_string(),
        provider_name,
        endpoint,
    }, deeplink))
}
```

Change `get_local_access_key` and `import_relay_pool_to_ccswitch` to call `ensure_secure_local_access_key()`. Change `AppDatabase::get_local_access_key` to `pub(crate)`; it remains available to in-crate migration and regression code but is not a public user-facing read path.

- [ ] **Step 4: Update and run the source contract**

Update `scripts/local-proxy-auth-contract.test.mjs` to assert that both public command paths use the ensuring method and register it in `scripts/run-contract-tests.mjs`.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml ccswitch_import_ensures_placeholder_key_before_building_deeplink -- --nocapture
node scripts/local-proxy-auth-contract.test.mjs
pnpm.cmd test:contracts
```

Expected: all commands exit 0; the deeplink contains the newly persisted key and never the placeholder.

- [ ] **Step 5: Commit only the key-contract hunks**

```powershell
git add -- src-tauri/src/commands/mod.rs scripts/local-proxy-auth-contract.test.mjs scripts/run-contract-tests.mjs
git add -p -- src-tauri/src/services/database.rs
git diff --cached --name-only
git diff --cached --check
git commit -m "fix: ensure local key before ccswitch export"
```

Expected staged paths: the four paths above; no unrelated capability-default hunk from `database.rs`.

## Task 2: Freeze Legacy Gateway Behavior

**Files:**
- Create: `src-tauri/src/services/proxy/contract_tests.rs`
- Create: `src-tauri/src/services/proxy/test_support.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs` only for narrow `pub(crate)` test seams

- [ ] **Step 1: Add a reusable loopback fixture**

Create a test-only fixture that always installs deadlines and joins its server thread:

```rust
pub(crate) struct LoopbackUpstream {
    pub base_url: String,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LoopbackUpstream {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("loopback upstream joins");
        }
    }
}
```

Expose `ScriptedResponse::{Json, Status, DelayedHeaders, ChunkedSse, DisconnectAfterChunk}` and `LoopbackUpstream::script(Vec<ScriptedResponse>)`. Capture every complete request as `CapturedRequest { method, path_and_query, headers, body }` through a channel. Every accepted socket gets a two-second test read/write timeout, and `finish(self)` joins the thread and returns the captured requests.

- [ ] **Step 2: Write characterization tests**

Add tests with these exact names:

```rust
#[test]
fn legacy_contract_authenticates_models_chat_responses_embeddings() {
    let mut case = LegacyGatewayCase::new();
    let local_key = case.local_key().to_string();
    for endpoint in [
        EndpointProbe::Models,
        EndpointProbe::Chat,
        EndpointProbe::Responses,
        EndpointProbe::Embeddings,
    ] {
        assert_eq!(case.request(endpoint, None).status, 401);
        assert_eq!(case.request(endpoint, Some("wrong")).status, 401);
        assert!(case.request(endpoint, Some(&local_key)).status < 400);
    }
    assert_eq!(case.upstream_requests(), 4);
}

#[test]
fn legacy_contract_preserves_query_and_safe_headers() {
    let case = LegacyGatewayCase::new();
    let observed = case.post_chat(
        "?beta=true",
        [("accept", "text/event-stream"), ("openai-project", "project-1")],
        br#"{"model":"alias-model","stream":true}"#,
    );
    assert_eq!(observed.upstream.path_and_query, "/v1/chat/completions?beta=true");
    assert_eq!(observed.upstream.body, br#"{"model":"mapped-model","stream":true}"#);
    assert_eq!(observed.upstream.header("accept"), Some("text/event-stream"));
    assert_eq!(observed.upstream.header("openai-project"), Some("project-1"));
    assert_eq!(observed.upstream.header("authorization"), Some("Bearer upstream-key"));
    assert_ne!(observed.upstream.header("authorization"), Some(case.local_key()));
}

#[test]
fn legacy_contract_fails_over_retryable_statuses_before_output() {
    for first_status in [429, 500] {
        let observed = LegacyGatewayCase::with_statuses([first_status, 200])
            .post_buffered_chat();
        assert_eq!(observed.downstream_status, 200);
        assert_eq!(observed.attempted_key_ids, ["key-a", "key-b"]);
        assert_eq!(observed.selected_key_id, "key-b");
        assert_eq!(observed.fallback_count, 1);
        assert_eq!(observed.health_updates, [("key-a", false), ("key-b", true)]);
        assert_eq!(observed.request_logs.len(), 1);
    }
}

#[test]
fn legacy_contract_current_raw_404_behavior_is_explicit() {
    let observed = LegacyGatewayCase::with_statuses([404, 200]).post_buffered_chat();
    assert_eq!(observed.downstream_status, 200);
    assert_eq!(observed.attempted_key_ids, ["key-a", "key-b"]);
    assert_eq!(observed.request_logs.len(), 1);
}

#[test]
fn legacy_contract_never_fails_over_after_stream_output() {
    let observed = LegacyGatewayCase::stream_then_disconnect(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"one\"}}]}\n\n",
    ).post_streaming_chat();
    assert!(observed.downstream_body.starts_with(b"data:"));
    assert_eq!(observed.attempted_key_ids, ["key-a"]);
    assert_eq!(observed.second_upstream_requests, 0);
    assert_eq!(observed.request_logs.len(), 1);
}

#[test]
fn legacy_contract_update_drain_tracks_active_stream() {
    let case = LegacyGatewayCase::paused_stream();
    let stream = case.start_streaming_chat();
    assert_eq!(case.status().active_requests, 1);
    let drain = case.begin_update_drain(Duration::from_secs(2));
    assert!(!drain.is_finished());
    stream.release_eof();
    assert!(drain.join().expect("drain thread").is_ok());
    assert_eq!(case.status().active_requests, 0);
    assert_eq!(case.request_logs().len(), 1);
}
```

Implement `LegacyGatewayCase` in `test_support.rs` as the sole setup helper: it creates an in-memory database, two Station Keys with deterministic IDs, starts `ProxyRuntimeState` on port 0, sends real loopback HTTP requests, exposes sanitized observations, and stops the runtime in `Drop`. In each result, additionally assert the expected route reason; the assertions above already lock status, selected Station Key, fallback count, upstream path/body, final health target, and one request-log row.

- [ ] **Step 3: Run the characterization suite**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml legacy_contract_ -- --nocapture
```

Expected: tests pass against current behavior except a known defect already represented by a focused failing regression. Do not weaken assertions to accommodate nondeterminism; fix fixture deadlines and ordering.

- [ ] **Step 4: Run the existing proxy suite**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
```

Expected: no new failures beyond Task 0 baseline.

- [ ] **Step 5: Commit the test harness**

```powershell
git add -- src-tauri/src/services/proxy/contract_tests.rs src-tauri/src/services/proxy/test_support.rs src-tauri/src/services/proxy/mod.rs
git add -p -- src-tauri/src/services/proxy/runtime.rs
git diff --cached --check
git commit -m "test: freeze local gateway behavior"
```

## Task 3: Add Typed Failures, Limits, and Body Budget

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/services/proxy/error.rs`
- Create: `src-tauri/src/services/proxy/limits.rs`
- Create: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write RED tests for defaults and weighted leases**

```rust
#[tokio::test]
async fn body_budget_holds_bytes_until_last_request_owner_drops() {
    let budget = BodyBudget::new(4 * 1024);
    let lease = budget.acquire(3072).await.expect("lease");
    let clone = lease.clone();
    drop(lease);
    assert_eq!(budget.available_bytes(), 1024);
    drop(clone);
    assert_eq!(budget.available_bytes(), 4096);
}

#[test]
fn proxy_server_limits_match_the_approved_budget() {
    let limits = ProxyServerLimits::default();
    assert_eq!(limits.max_connections, 64);
    assert_eq!(limits.max_in_flight_requests, 32);
    assert_eq!(limits.max_header_bytes, 64 * 1024);
    assert_eq!(limits.max_body_bytes, 32 * 1024 * 1024);
    assert_eq!(limits.max_buffered_body_bytes, 128 * 1024 * 1024);
    assert_eq!(limits.header_timeout, Duration::from_secs(10));
    assert_eq!(limits.body_timeout, Duration::from_secs(30));
    assert_eq!(limits.upstream_connect_timeout, Duration::from_secs(10));
    assert_eq!(limits.upstream_first_byte_timeout, Duration::from_secs(120));
    assert_eq!(limits.precommit_timeout, Duration::from_secs(180));
    assert_eq!(limits.buffered_execution_timeout, Duration::from_secs(300));
    assert_eq!(limits.stream_idle_timeout, Duration::from_secs(90));
    assert_eq!(limits.shutdown_timeout, Duration::from_secs(30));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml body_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml proxy_server_limits_ -- --nocapture
```

Expected: FAIL because the modules and types do not exist.

- [ ] **Step 3: Add the async HTTP dependencies**

Use compatible major versions and disable Reqwest defaults:

```toml
axum = { version = "0.8", default-features = false, features = ["http1", "json", "tokio"] }
bytes = "1"
futures-util = "0.3"
http = "1"
http-body-util = "0.1"
http-body = "1"
hyper = { version = "1", features = ["http1", "server"] }
hyper-util = { version = "0.1", features = ["tokio"] }
reqwest = { version = "0.13", default-features = false, features = ["rustls", "stream", "json", "socks"] }
subtle = "2"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "sync", "time"] }
tokio-util = { version = "0.7", features = ["rt"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors", "limit", "request-id"] }
```

Keep `ureq` and `httparse` during migration. Confirm Cargo resolves one Reqwest 0.13 version.

- [ ] **Step 4: Implement the types**

`error.rs` defines `ProxyFailureCode`, `FailureSource`, `RetryClass`, and `ProxyFailure::into_response()`. `limits.rs` defines the exact approved defaults and a weighted `BodyBudgetLease` backed by `tokio::sync::Semaphore`. One permit represents 1 KiB, so the aggregate 128 MiB budget uses 131,072 permits. `BodyBudgetLease` wraps an `Arc<OwnedSemaphorePermit>` so cloning a request shares one reservation instead of reserving or copying the body again.

`limits.rs` also defines `RequestLease`. Ingress acquires a request permit before reading the body; failure returns `local_proxy_busy`. Task 11 adds the bounded finalization-slot reservation when real responses are wired. `request.rs` defines the shared response type and keeps the resource leases private:

```rust
pub enum ProxyResponsePayload {
    Buffered(Bytes),
    Stream(ByteStream),
}

pub struct ProxyHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub payload: ProxyResponsePayload,
    pub outcome: FinalRequestOutcome,
}
```

During Task 3, define `ByteStream` as a crate-local boxed stream type in `request.rs`; Task 9 moves its alias to `upstream.rs` without changing consumers. Define `FinalRequestOutcome` initially in `request.rs`; Task 7 moves it to `routing_repository.rs`. This is a mechanical ownership move, not a second type.

`CanonicalProxyRequest` keeps the three leases private:

```rust
pub struct CanonicalProxyRequest {
    pub request_id: String,
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub requirements: RequestRequirements,
    pub body: Bytes,
    pub forwarded_headers: HeaderMap,
    pub idempotency_key: Option<String>,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
    _body_budget: BodyBudgetLease,
    _request_lease: RequestLease,
}
```

`RequestLease` owns the in-flight semaphore permit plus an active-request counter guard. Both leases move from ingress into the response finalizer and remain live until response EOF/error/drop.

- [ ] **Step 5: Run GREEN and dependency checks**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml body_budget_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml proxy_server_limits_ -- --nocapture
cargo tree --manifest-path src-tauri/Cargo.toml -d | Select-String "reqwest v0\."
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: tests and check pass; duplicate output does not show both Reqwest 0.12 and 0.13.

- [ ] **Step 6: Commit foundations**

```powershell
git add -- src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/services/proxy/error.rs src-tauri/src/services/proxy/limits.rs src-tauri/src/services/proxy/request.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "feat: add bounded proxy foundations"
```

## Task 4: Isolate the Legacy Runtime Behind a Facade

**Files:**
- Rename: `src-tauri/src/services/proxy/runtime.rs` -> `src-tauri/src/services/proxy/legacy_runtime.rs`
- Create: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `scripts/local-proxy-auth-contract.test.mjs`
- Modify: `scripts/request-cost-model-pricing.test.mjs`

- [ ] **Step 1: Add a failing facade contract**

Add a source contract requiring `runtime.rs` to stay lifecycle-only and the old symbols to live in `legacy_runtime.rs`:

```js
assert.match(runtime, /pub struct ProxyRuntimeState/);
assert.doesNotMatch(runtime, /fn forward_(chat|responses|embeddings)_request/);
assert.match(legacyRuntime, /fn forward_responses_request/);
```

Run `node scripts/local-proxy-auth-contract.test.mjs`; expect FAIL before the move.

- [ ] **Step 2: Mechanically move the file and re-export the legacy type**

Use a mechanical rename, then create this narrow facade:

```rust
pub use super::legacy_runtime::ProxyRuntimeState;
```

Declare `mod legacy_runtime;` in `mod.rs`. Update source contracts to inspect `legacy_runtime.rs` for legacy behavior while keeping public imports at `runtime::ProxyRuntimeState`. Do not alter forwarding behavior in this task.

- [ ] **Step 3: Prove behavior and symbol compatibility**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
node scripts/local-proxy-auth-contract.test.mjs
node scripts/request-cost-model-pricing.test.mjs
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all commands match Task 0 baseline or better; Tauri command imports require no changes.

- [ ] **Step 4: Commit the mechanical boundary**

```powershell
git add -- src-tauri/src/services/proxy/legacy_runtime.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/mod.rs scripts/local-proxy-auth-contract.test.mjs scripts/request-cost-model-pricing.test.mjs
git diff --cached --check
git commit -m "refactor: isolate legacy proxy runtime"
```

## Task 5: Build the Authenticated V2 Ingress Service

**Files:**
- Modify: `src-tauri/src/services/proxy/local_auth.rs`
- Create: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write service-level RED tests**

Add tests in `ingress.rs` using `tower::ServiceExt::oneshot` and an in-memory `IngressState`. The initial handler may return `501 v2_execution_not_wired` after successful canonicalization.

```rust
#[tokio::test]
async fn ingress_requires_auth_and_returns_request_id() {
    let app = test_router(test_state());
    let missing = app.clone().oneshot(request("POST", "/v1/responses", None, b"{}")).await.unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(missing).await, "local_auth_missing");

    let accepted = app.oneshot(request("POST", "/v1/responses", Some("relay-local-secret"), br#"{"model":"gpt-test"}"#)).await.unwrap();
    assert_eq!(accepted.status(), StatusCode::NOT_IMPLEMENTED);
    assert!(accepted.headers().contains_key("x-relay-request-id"));
}

#[tokio::test]
async fn ingress_enforces_body_and_global_memory_limits() {
    let state = test_state_with_limits(ProxyServerLimits {
        max_body_bytes: 4,
        max_buffered_body_bytes: 4,
        ..test_limits()
    });
    let app = test_router(state);
    let too_large = app.clone().oneshot(request("POST", "/v1/responses", Some("relay-local-secret"), b"12345")).await.unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(too_large).await, "request_body_too_large");
}
```

Also test `OPTIONS`, loopback/non-loopback Origin, `/usage`, `/v1/usage`, `/v1/models`, all three POST endpoints, 404, 405, body timeout, and safe-header allowlisting.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml ingress_ -- --nocapture
```

Expected: FAIL because `ingress.rs`, `IngressState`, and `test_router` do not exist.

- [ ] **Step 3: Make local auth accept `HeaderMap` with `subtle`**

Replace the map-specific signature with this production boundary:

```rust
pub fn authorize_headers(headers: &http::HeaderMap, configured_key: &str) -> AuthDecision {
    let Some(value) = headers.get(http::header::AUTHORIZATION) else {
        return AuthDecision::Missing;
    };
    let Ok(value) = value.to_str() else {
        return AuthDecision::Invalid;
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")) else {
        return AuthDecision::Invalid;
    };
    let left = token.trim().as_bytes();
    let right = configured_key.trim().as_bytes();
    if left.len() == right.len() && bool::from(left.ct_eq(right)) {
        AuthDecision::Accepted
    } else {
        AuthDecision::Invalid
    }
}
```

Use `subtle::ConstantTimeEq`. Preserve loopback-origin tests.

- [ ] **Step 4: Implement the ingress router**

Create one wildcard handler so method/path matching stays centralized:

```rust
pub fn router(state: Arc<IngressState>) -> Router {
    Router::new()
        .route("/usage", get(handle))
        .route("/v1/usage", get(handle))
        .route("/v1/models", get(handle))
        .route("/v1/chat/completions", post(handle))
        .route("/v1/responses", post(handle))
        .route("/v1/embeddings", post(handle))
        .fallback(not_found)
        .with_state(state)
}
```

`handle` performs, in order: request permit, request id, CORS, auth, endpoint match, bounded body collection, JSON metadata extraction, safe-header copy, then calls the injected `IngressExecutor`. Known `Content-Length` reserves its KiB weight before polling the body; unknown/chunked bodies acquire additional KiB permits before appending each chunk. The 32 MiB per-request check occurs before allocation/growth. Define the test seam as:

```rust
pub trait IngressExecutor: Send + Sync {
    fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> futures_util::future::BoxFuture<'static, Result<ProxyHttpResponse, ProxyFailure>>;
}
```

Keep this trait crate-local and implement it with boxed futures; do not add `async-trait` and do not expose a plugin API.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml ingress_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::local_auth -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all pass. Pre-parse Hyper failures are not asserted here because these tests begin after an HTTP request exists.

- [ ] **Step 6: Commit ingress**

```powershell
git add -- src-tauri/src/services/proxy/local_auth.rs src-tauri/src/services/proxy/ingress.rs src-tauri/src/services/proxy/request.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "feat: add authenticated v2 proxy ingress"
```

## Task 6: Add the Tokio/Hyper Server and Lifecycle State Machine

**Files:**
- Create: `src-tauri/src/services/proxy/server.rs`
- Replace: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/commands/mod.rs:395-485`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src/lib/types/proxy.ts`

- [ ] **Step 1: Write lifecycle RED tests**

```rust
#[tokio::test]
async fn v2_runtime_transitions_start_run_drain_stop() {
    let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
    let started = runtime.start(test_start_config(0)).await.expect("start");
    assert_eq!(started.lifecycle, ProxyLifecycle::Running);
    assert_ne!(started.port, 0);

    let draining = runtime.prepare_for_update(Duration::from_millis(250)).await.expect("drain");
    assert_eq!(draining.lifecycle, ProxyLifecycle::Stopped);
    assert!(!draining.running);
}

#[tokio::test]
async fn v2_runtime_reports_bind_failure_and_recovers() {
    let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
    assert!(runtime.start(test_start_config(port)).await.is_err());
    assert_eq!(runtime.status(port).lifecycle, ProxyLifecycle::Failed);
    drop(occupied);
    assert_eq!(runtime.start(test_start_config(port)).await.unwrap().lifecycle, ProxyLifecycle::Running);
}
```

Add tests for idempotent same-port start, different-port restart requirement, 64-connection admission, 32-request admission, active body tracking, forced shutdown, JoinError -> Failed, and zero-active stop under one second. The 65th raw connection is a pre-request transport overload: assert that it is closed immediately without an HTTP response and without spawning a connection task. The 33rd parsed request must receive `503 local_proxy_busy` from Axum.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_runtime_ -- --nocapture
```

Expected: FAIL because V2 runtime and lifecycle variants do not exist.

- [ ] **Step 3: Extend lifecycle types compatibly**

```rust
pub enum ProxyLifecycle {
    Stopped,
    Starting,
    Running,
    Draining,
    Stopping,
    Failed,
}
```

Mirror the union in `src/lib/types/proxy.ts`. Keep all existing `ProxyStatus` fields and semantics.

- [ ] **Step 4: Implement the server handle**

`server.rs` owns this explicit handle:

```rust
pub struct RunningServer {
    pub local_addr: SocketAddr,
    pub cancel: CancellationToken,
    pub active_requests: Arc<AtomicU32>,
    pub request_count: Arc<AtomicU64>,
    join: JoinHandle<Result<(), String>>,
}
```

`spawn_server` binds `127.0.0.1`, attempts to acquire a connection semaphore after accept and before spawning a Hyper HTTP/1.1 connection, applies the header read timeout, and tracks every admitted connection in a `JoinSet`. When the 64 permits are occupied, close the newly accepted socket immediately; do not spawn a task per rejected connection. Cancellation stops accept, requests graceful connection shutdown, then waits for tracked tasks. Do not detach connection tasks.

- [ ] **Step 5: Implement the mode-owning facade**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRuntimeMode { Legacy, V2 }

pub struct ProxyRuntimeState {
    mode: ProxyRuntimeMode,
    legacy: legacy_runtime::ProxyRuntimeState,
    v2: tokio::sync::Mutex<V2RuntimeInner>,
    lifecycle_operation: tokio::sync::Mutex<()>,
    status_snapshot: std::sync::RwLock<ProxyStatus>,
}
```

`Default` selects Legacy through Task 16. `for_tests(mode)` injects the choice without environment variables. Only `from_environment_for_dev()` reads `RELAY_POOL_PROXY_RUNTIME`. `status(default_port)` remains synchronous and reads `status_snapshot`; async lifecycle transitions publish a new snapshot after each state change.

Define the start input in this task:

```rust
pub struct ProxyStartConfig {
    pub database: AppDatabase,
    pub data_key: [u8; 32],
    pub port: u16,
    pub limits: ProxyServerLimits,
}

impl ProxyStartConfig {
    pub fn new(database: AppDatabase, data_key: [u8; 32], port: u16) -> Self {
        Self { database, data_key, port, limits: ProxyServerLimits::default() }
    }
}
```

- [ ] **Step 6: Convert lifecycle and import commands to async**

Keep command names and returned JSON unchanged:

```rust
#[tauri::command]
pub async fn start_local_proxy(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(secrets.data_key())?;
    proxy.start(ProxyStartConfig::new(database.inner().clone(), *secrets.data_key(), settings.local_proxy_port)).await
}
```

Apply the same pattern to stop, cleanup, prepare, restart, and `import_relay_pool_to_ccswitch`. Keep `get_proxy_status`, workspace loads, and reorder commands synchronous because `status()` is a snapshot read. Legacy lifecycle calls go through `spawn_blocking`; never block a Tokio worker on `JoinHandle::join()` or the SQLite mutex.

- [ ] **Step 7: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_runtime_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml prepare_for_update -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
pnpm.cmd build
```

Expected: lifecycle and compatibility checks pass; frontend typecheck accepts the expanded union.

- [ ] **Step 8: Commit lifecycle**

```powershell
git add -- src-tauri/src/services/proxy/server.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/models/proxy.rs src-tauri/src/commands/mod.rs src-tauri/src/services/proxy/mod.rs src/lib/types/proxy.ts
git diff --cached --check
git commit -m "feat: add async proxy lifecycle"
```

## Task 7: Introduce the Async Routing Repository Boundary

**Files:**
- Create: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/database.rs:1295-1316,1594-1598,2167-2204`

- [ ] **Step 1: Write repository RED tests**

```rust
#[tokio::test]
async fn repository_loads_runtime_candidates_without_blocking_async_callers() {
    let database = seeded_database();
    let repository = SqliteRoutingRepository::new(database.clone(), test_data_key());
    let candidates = repository.load_candidates().await.expect("candidates");
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|candidate| !candidate.api_key.is_empty()));
}

#[tokio::test]
async fn repository_records_one_final_outcome_for_endpoint_revision() {
    let database = seeded_database();
    let repository = SqliteRoutingRepository::new(database.clone(), test_data_key());
    repository.finalize(test_final_outcome()).await.expect("finalize");
    assert_eq!(database.list_local_proxy_request_logs().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml repository_ -- --nocapture
```

Expected: FAIL because the repository does not exist.

- [ ] **Step 3: Define the sealed repository interface**

```rust
pub trait RoutingRepository: Send + Sync {
    fn load_candidates(&self) -> BoxFuture<'static, Result<Vec<RichRouteCandidate>, ProxyFailure>>;
    fn load_settings(&self) -> BoxFuture<'static, Result<RoutingRuntimeSettings, ProxyFailure>>;
    fn record_attempt_failure<'a>(
        &'a self,
        candidate: &'a RichRouteCandidate,
        failure: &'a ProxyFailure,
    ) -> BoxFuture<'a, Result<(), ProxyFailure>>;
    fn finalize(&self, outcome: FinalRequestOutcome) -> BoxFuture<'static, Result<(), ProxyFailure>>;
}
```

The concrete implementation clones `AppDatabase` and wraps every SQLite call in `tauri::async_runtime::spawn_blocking`. No `MutexGuard<Connection>` may exist across `.await`. Keep existing `AppDatabase` methods for UI and legacy consumers; add only narrow crate-private helpers needed for atomic finalization.

- [ ] **Step 4: Make finalization transactional where required**

Add one database method that applies health feedback, endpoint-revision guard, and request-log insert in a transaction when they must agree. It accepts `FinalRequestOutcome`; it never accepts a transport object or secret.

The transaction first inserts the request log with `ON CONFLICT(request_id) DO NOTHING`. It applies health/affinity feedback only when that insert affected one row. A repeated `request_id` commits no feedback, making the whole finalization idempotent rather than only deduplicating logs.

- [ ] **Step 5: Run GREEN and DB regressions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml repository_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml record_station_key_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml request_log_ -- --nocapture
```

Expected: all pass; runtime candidate types contain decrypted keys, UI read types do not.

- [ ] **Step 6: Commit repository hunks only**

```powershell
git add -- src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/services/proxy/mod.rs
git add -p -- src-tauri/src/services/database.rs
git diff --cached --check
git commit -m "refactor: isolate proxy routing persistence"
```

## Task 8: Create Sealed Endpoint Adapters

**Files:**
- Create: `src-tauri/src/services/proxy/endpoint_adapter.rs`
- Modify: `src-tauri/src/services/proxy/responses_chat_fallback.rs`
- Modify: `src-tauri/src/services/proxy/adapters/openai.rs`
- Modify: `src-tauri/src/services/proxy/adapters/responses.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write adapter RED tests**

```rust
#[test]
fn endpoint_adapters_prepare_exact_paths_models_and_headers() {
    let request = canonical_request(RouteEndpointKind::Responses, br#"{"model":"client-model","input":"hi"}"#);
    let candidate = candidate(UpstreamApiFormat::OpenAiResponses, "upstream-model");
    let prepared = EndpointAdapter::Responses.prepare(&request, &candidate).expect("prepared");
    assert_eq!(prepared.path, "/v1/responses");
    assert_eq!(serde_json::from_slice::<Value>(&prepared.body).unwrap()["model"], "upstream-model");
    assert_eq!(prepared.headers.get(ACCEPT).unwrap(), "application/json");
}

#[test]
fn endpoint_adapters_reject_unsupported_streaming_chat_bridge() {
    let request = streaming_responses_request();
    let candidate = candidate(UpstreamApiFormat::OpenAiChatCompletions, "gpt-test");
    let failure = EndpointAdapter::Responses.prepare(&request, &candidate).unwrap_err();
    assert_eq!(failure.code, ProxyFailureCode::ResponsesChatFallbackIncompatible);
}
```

Also cover Models, Embeddings, Chat, direct Responses, automatic Responses-to-Chat buffered fallback, header allowlist, and response-header allowlist.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml endpoint_adapters_ -- --nocapture
```

Expected: FAIL because `EndpointAdapter` does not exist.

- [ ] **Step 3: Implement the sealed enum**

```rust
pub enum EndpointAdapter { Models, Embeddings, ChatCompletions, Responses }

pub struct PreparedUpstreamRequest {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub response_mode: ResponseMode,
}
```

The enum is crate-private and selected only from `RouteEndpointKind`. Move model rewrite/path/header behavior from legacy runtime into pure adapter functions without changing legacy callers. Do not add a registry.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml endpoint_adapters_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml responses_chat_fallback_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::adapters -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Commit adapters**

```powershell
git add -- src-tauri/src/services/proxy/endpoint_adapter.rs src-tauri/src/services/proxy/responses_chat_fallback.rs src-tauri/src/services/proxy/adapters/openai.rs src-tauri/src/services/proxy/adapters/responses.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "refactor: seal proxy endpoint adapters"
```

## Task 9: Build the Shared Reqwest Upstream Transport

**Files:**
- Create: `src-tauri/src/services/proxy/upstream.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Test: `src-tauri/src/services/proxy/test_support.rs`

- [ ] **Step 1: Write transport RED tests**

```rust
#[tokio::test]
async fn upstream_transport_reuses_clients_and_never_follows_redirects() {
    let pool = UpstreamClientPool::new(test_limits()).expect("pool");
    assert!(Arc::ptr_eq(&pool.client(&ProxyRoute::Direct).unwrap(), &pool.client(&ProxyRoute::Direct).unwrap()));
    let upstream = redirecting_upstream("https://other.example/secret");
    let outcome = pool.send(prepared_request(&upstream.base_url), &test_candidate()).await.unwrap();
    assert_eq!(outcome.status, StatusCode::FOUND);
}

#[tokio::test]
async fn upstream_transport_classifies_connect_timeout_and_http_status() {
    let pool = UpstreamClientPool::new(short_limits()).unwrap();
    let connect = pool.send(prepared_request("http://127.0.0.1:9"), &test_candidate()).await.unwrap_err();
    assert_eq!(connect.code, ProxyFailureCode::UpstreamConnectFailed);
    let status = pool.send(prepared_request(&status_upstream(429).base_url), &test_candidate()).await.unwrap();
    assert_eq!(status.status, StatusCode::TOO_MANY_REQUESTS);
}
```

Add HTTP proxy and SOCKS URL validation tests without requiring an external proxy service.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml upstream_transport_ -- --nocapture
```

Expected: FAIL because the pool and outcome types do not exist.

- [ ] **Step 3: Implement a snapshot client pool**

```rust
pub struct UpstreamClientPool {
    direct: Arc<reqwest::Client>,
    proxied: tokio::sync::RwLock<HashMap<ProxyRoute, Arc<reqwest::Client>>>,
    limits: ProxyServerLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProxyRoute {
    Direct,
    Http(String),
    Socks(String),
}

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, ProxyFailure>> + Send>>;

pub enum UpstreamAttempt {
    Buffered { status: StatusCode, headers: HeaderMap, body: Bytes },
    Stream { status: StatusCode, headers: HeaderMap, chunks: ByteStream },
}
```

Use `tokio::sync::RwLock<HashMap<...>>`; do not add `dashmap`. Build clients with redirects disabled, connect timeout set, no cookie store, and no default authorization. Inject the candidate secret only into the individual request.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml upstream_transport_ -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all pass; repeated direct requests reuse one client.

- [ ] **Step 5: Commit transport**

```powershell
git add -- src-tauri/src/services/proxy/upstream.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/test_support.rs
git diff --cached --check
git commit -m "feat: add shared async upstream transport"
```

## Task 10: Implement the Unified Retry Policy and Execution Engine

**Files:**
- Create: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/routing_failure.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/legacy_runtime.rs`

- [ ] **Step 1: Write table-driven RED tests for every retry class**

```rust
#[test]
fn retry_policy_matches_the_approved_precommit_matrix() {
    let cases = [
        (failure(401, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (failure(403, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (capability_mismatch(404), false, false, RetryDecision::NextCandidate),
        (failure(404, FailureSource::UpstreamHttp), false, false, RetryDecision::Stop),
        (failure(408, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (failure(425, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (failure(429, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (failure(500, FailureSource::UpstreamHttp), false, false, RetryDecision::NextCandidate),
        (failure(400, FailureSource::UpstreamHttp), false, false, RetryDecision::Stop),
        (failure(409, FailureSource::UpstreamHttp), false, false, RetryDecision::Stop),
        (failure(422, FailureSource::UpstreamHttp), false, false, RetryDecision::Stop),
        (ambiguous_transport_failure(), false, false, RetryDecision::Stop),
        (ambiguous_transport_failure(), true, false, RetryDecision::NextCandidate),
        (stream_failure(), true, true, RetryDecision::Stop),
    ];
    for (failure, idempotent, committed, expected) in cases {
        assert_eq!(RetryPolicy::default().decide(&failure, idempotent, committed), expected);
    }
}
```

Add tests for maximum three distinct candidates, 180-second precommit budget, 300-second buffered budget, no same-candidate retry, ready-candidate preference over `Retry-After` waiting, and existing sticky/fallback wait behavior.

- [ ] **Step 2: Write an engine RED test with fakes**

```rust
#[tokio::test]
async fn execution_engine_preserves_route_order_and_finalizes_one_candidate() {
    let repository = FakeRepository::with_candidates([candidate("a"), candidate("b")]);
    let attempts = FakeAttemptExecutor::responses([http_failure(429), buffered_success(b"{\"ok\":true}")]);
    let engine = ExecutionEngine::new(repository.clone(), attempts.clone(), test_clock());

    let response = engine.execute(canonical_chat_request()).await.expect("response");

    assert_eq!(attempts.seen_ids(), ["a", "b"]);
    assert_eq!(response.selected_station_key_id(), Some("b"));
    assert_eq!(response.fallback_count(), 1);
    assert_eq!(repository.finalized_count(), 0, "response body owns finalization");
}
```

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml retry_policy_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml execution_engine_ -- --nocapture
```

Expected: FAIL because the new policy and engine do not exist.

- [ ] **Step 4: Implement one route/attempt loop**

```rust
pub struct ExecutionEngine {
    repository: Arc<dyn RoutingRepository>,
    attempts: Arc<dyn AttemptExecutor>,
    retry_policy: RetryPolicy,
    scheduler: Arc<SchedulerRuntimeState>,
}

pub trait AttemptExecutor: Send + Sync {
    fn attempt<'a>(
        &'a self,
        request: &'a CanonicalProxyRequest,
        candidate: &'a RichRouteCandidate,
    ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>>;
}

pub async fn execute(
    &self,
    request: CanonicalProxyRequest,
) -> Result<ProxyExecutionResponse, ProxyFailure> {
    let plan = self.plan(&request).await?;
    let deadline = Instant::now() + self.retry_policy.budget_for(request.stream);
    for (index, candidate) in plan.candidates.iter().take(3).enumerate() {
        let outcome = self.attempts.attempt(&request, candidate).await;
        match outcome {
            Ok(prepared) => return Ok(self.prepare_response(request, plan, candidate, index, prepared)),
            Err(failure) if self.retry_policy.may_continue(&request, &failure, deadline, index) => {
                self.repository.record_attempt_failure(candidate, &failure).await?;
            }
            Err(failure) => return Err(failure.with_route_plan(&plan, index)),
        }
    }
    Err(ProxyFailure::all_candidates_failed(&plan))
}
```

`ExecutionEngine` implements the crate-local `IngressExecutor`. The implementation maps `ProxyExecutionResponse` into `ProxyHttpResponse` exactly once; ingress never inspects route candidates or retry state.

The real code must preserve candidate order, existing capacity waits, affinity, probe confirmation, endpoint revision, route explanation, and candidate-specific feedback. It must not finalize request success before the response body completes.

- [ ] **Step 5: Align the shared classifier**

Update `routing_failure.rs` so 401/403 mean hard failure for that candidate but remain retryable before output. Add 408, 425, 409, and 422. A raw 404 stops; only an adapter-produced `CapabilityMismatch` classification may fail over. Replace `legacy_contract_current_raw_404_behavior_is_explicit` with `legacy_contract_fails_over_only_classified_404_capability_mismatch`: one raw-404 case must stop after key A, and one adapter-classified capability mismatch must reach key B. Change legacy fallback decisions to call `RetryPolicy`/the shared classifier rather than `should_fallback` status shortcuts.

- [ ] **Step 6: Run GREEN and legacy regressions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml retry_policy_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml execution_engine_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::routing_failure -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_contract_ -- --nocapture
```

Expected: all pass; 401/403 fail over before output but still damage only the actual candidate.

- [ ] **Step 7: Commit the execution core**

```powershell
git add -- src-tauri/src/services/proxy/execution.rs src-tauri/src/services/proxy/routing_failure.rs src-tauri/src/services/proxy/mod.rs
git add -p -- src-tauri/src/services/proxy/legacy_runtime.rs
git diff --cached --check
git commit -m "feat: unify proxy candidate execution"
```

## Task 11: Wire Buffered Models, Usage, Embeddings, Chat, and Responses

**Files:**
- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/endpoint_adapter.rs`
- Modify: `src-tauri/src/services/proxy/upstream.rs`
- Create: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/server.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/contract_tests.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write buffered endpoint RED tests against a real v2 listener**

```rust
#[tokio::test]
async fn v2_buffered_endpoints_match_the_legacy_contract() {
    let fixture = BufferedParityFixture::start().await;
    for case in fixture.cases(["/v1/models", "/usage", "/v1/usage", "/v1/embeddings", "/v1/chat/completions", "/v1/responses"]) {
        let legacy = case.run(ProxyRuntimeMode::Legacy).await;
        let v2 = case.run(ProxyRuntimeMode::V2).await;
        assert_eq!(comparable_response(v2), comparable_response(legacy), "{}", case.path);
        assert_eq!(v2.selected_station_key_id, legacy.selected_station_key_id);
        assert_eq!(v2.route_reason, legacy.route_reason);
    }
}
```

Add explicit tests for model alias rewrite, query preservation, safe headers, Responses direct output, Responses buffered Chat bridge, fallback count, and unsupported endpoint 404/405.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_buffered_ -- --nocapture
```

Expected: FAIL because v2 still returns `v2_execution_not_wired`.

- [ ] **Step 3: Wire local read endpoints**

Implement `/usage` and `/v1/usage` through repository reads without entering candidate attempts. Implement `/v1/models` through the existing aggregation behavior and preserve its response shape/order. All still pass auth/admission/request-id handling.

- [ ] **Step 4: Wire routed buffered endpoints**

Connect ingress -> `ExecutionEngine` -> `EndpointAdapter` -> `UpstreamClientPool` for Embeddings, Chat, and Responses. Enforce `buffered_execution_timeout` around the entire multi-candidate operation, not per candidate. Preserve response-header allowlist and redact upstream errors.

- [ ] **Step 5: Add the minimal bounded finalization path**

`server.rs` starts a bounded `FinalizationDispatcher` with capacity `max_in_flight_requests`. Ingress reserves one owned sender permit before calling the real executor; failure returns `local_proxy_busy`. `ExecutionEngine` moves the permit, request/body leases, and pending `FinalRequestOutcome` into `FinalizingBody`. The initial body implementation must finalize buffered EOF and early drop exactly once; Task 13 adds exhaustive error/cancellation/shutdown coverage.

- [ ] **Step 6: Run GREEN and parity**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_buffered_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml endpoint_adapters_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml upstream_transport_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml execution_engine_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_contract_ -- --nocapture
```

Expected: all pass; no buffered endpoint uses the legacy raw socket writer in v2 mode.

- [ ] **Step 7: Commit buffered routing**

```powershell
git add -- src-tauri/src/services/proxy/ingress.rs src-tauri/src/services/proxy/execution.rs src-tauri/src/services/proxy/endpoint_adapter.rs src-tauri/src/services/proxy/upstream.rs src-tauri/src/services/proxy/response_body.rs src-tauri/src/services/proxy/server.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/contract_tests.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "feat: route buffered requests through v2"
```

## Task 12: Add Request Trace Fields and Transactional Final Outcomes

**Files:**
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/database.rs:388-419,2500-2550,7579-7595,11271-11430`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src/lib/types/proxy.ts`
- Modify: `src/features/logs/requestLogViewModels.ts`

- [ ] **Step 1: Write migration and finalization RED tests**

```rust
#[test]
fn request_log_migration_adds_v2_trace_columns_without_losing_rows() {
    let database = legacy_request_log_database();
    database.initialize_migrations().expect("migrate");
    let columns = request_log_columns(&database);
    for name in ["request_id", "body_bytes", "attempt_count", "route_wait_ms", "upstream_headers_ms", "failure_source", "attempts_json", "completion_source"] {
        assert!(columns.contains(name), "missing {name}");
    }
    assert_eq!(database.list_request_logs().unwrap().len(), 1);
}

#[tokio::test]
async fn final_outcome_writes_log_and_candidate_feedback_once() {
    let repository = seeded_repository();
    let outcome = test_final_outcome();
    repository.finalize(outcome.clone()).await.unwrap();
    repository.finalize(outcome).await.unwrap();
    assert_eq!(repository.request_log_count().await, 1);
    assert_eq!(repository.health_transition_count().await, 1);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml request_log_migration_adds_v2 -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml final_outcome_ -- --nocapture
```

Expected: FAIL because the columns and idempotent finalization key do not exist.

- [ ] **Step 3: Extend the schema compatibly**

Add nullable columns through `migrate_request_log_observability_columns`. Use `request_id TEXT` with a unique partial index for non-null values so repeated finalization becomes an idempotent no-op. Add matching optional fields to `RequestLog`, `CreateRequestLogInput`, `REQUEST_LOG_SELECT_COLUMNS`, row mapping, insert SQL, and TypeScript.

- [ ] **Step 4: Add sanitized attempt traces**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AttemptTrace {
    pub station_key_id: String,
    pub failure_code: Option<String>,
    pub duration_ms: i64,
}
```

`attempts_json` never contains upstream URL query, headers, body, key, cookie, or raw error. `failure_source` uses the stable enum string. Update the log view model to show request ID, attempt count, stage timings, and completion source in the existing detail area; do not redesign the page.

- [ ] **Step 5: Run GREEN and frontend checks**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml request_log_migration_adds_v2 -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml final_outcome_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml request_log_ -- --nocapture
pnpm.cmd test
pnpm.cmd build
```

Expected: all pass; old databases retain rows and new fields deserialize as null.

- [ ] **Step 6: Commit schema and trace fields**

```powershell
git add -- src-tauri/src/models/proxy.rs src-tauri/src/services/proxy/observability.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/services/proxy/execution.rs src/lib/types/proxy.ts src/features/logs/requestLogViewModels.ts
git add -p -- src-tauri/src/services/database.rs
git diff --cached --check
git commit -m "feat: persist proxy attempt traces"
```

## Task 13: Harden Response Finalization and Shutdown Delivery

**Files:**
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/server.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write body-lifecycle RED tests**

```rust
#[tokio::test]
async fn response_body_finalizes_success_only_after_eof() {
    let sink = RecordingFinalizer::default();
    let mut body = FinalizingBody::buffered(Bytes::from_static(b"ok"), sink.clone(), success_outcome());
    assert_eq!(sink.calls(), 0);
    assert_eq!(body.frame().await.unwrap().unwrap().into_data().unwrap(), b"ok"[..]);
    assert!(body.frame().await.is_none());
    assert_eq!(sink.calls(), 1);
    assert_eq!(sink.last().completion_source, CompletionSource::BufferedComplete);
}

#[tokio::test]
async fn response_body_drop_finalizes_downstream_disconnect_once() {
    let sink = RecordingFinalizer::default();
    let body = FinalizingBody::stream(pending_stream(), sink.clone(), pending_outcome());
    drop(body);
    tokio::task::yield_now().await;
    assert_eq!(sink.calls(), 1);
    assert_eq!(sink.last().completion_source, CompletionSource::DownstreamDropped);
}
```

Also test body error, double drop, cancellation, and repository failure without panic.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml response_body_ -- --nocapture
```

Expected: FAIL because `FinalizingBody` does not exist.

- [ ] **Step 3: Harden the owned finalizer guard**

Complete the Task 11 `FinalizingBody` implementation so `http_body::Body<Data = Bytes, Error = ProxyBodyError>` handles every terminal path. It owns an `Arc<FinalizeOnce>` whose atomic state permits one transition. EOF submits success; stream error submits the classified failure; `Drop` submits downstream disconnect. Each admitted request already owns an `mpsc::OwnedPermit<FinalRequestOutcome>` in `FinalizationLease`; terminal submission calls `permit.send(outcome)` synchronously, so `poll_frame` and `Drop` never block on SQLite and the queue cannot exceed `max_in_flight_requests`.

`server.rs` starts one tracked finalizer worker that drains the bounded channel and calls `RoutingRepository::finalize`. Shutdown first waits for active response bodies, then drops the last sender, then awaits the finalizer worker before reporting `Stopped`. A closed dispatcher before shutdown completion is a `Failed` lifecycle condition, not a silently dropped log. `RequestLease` and `BodyBudgetLease` release at body terminal state; the reserved finalization slot remains owned by the queued outcome until the worker receives it.

- [ ] **Step 4: Prove no handler-side finalization remains**

Search v2 ingress/execution for direct `RoutingRepository::finalize` calls and remove them. All buffered responses must already use `Body::new(FinalizingBody::buffered(...))`; active-request and body-budget guards remain owned until the wrapper terminates. Enforce this task with the Rust fake-repository test. Task 16 adds the permanent source-boundary assertion after creating `local-proxy-v2-boundary.test.mjs`.

- [ ] **Step 5: Run GREEN and leak loops**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml response_body_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml final_outcome_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml v2_buffered_ -- --nocapture
```

Expected: all pass; each request produces one final log and all counters return to zero.

- [ ] **Step 6: Commit body lifecycle**

```powershell
git add -- src-tauri/src/services/proxy/response_body.rs src-tauri/src/services/proxy/server.rs src-tauri/src/services/proxy/execution.rs src-tauri/src/services/proxy/ingress.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "feat: finalize proxy responses on body completion"
```

## Task 14: Implement Stream Bootstrap, Idle Timeout, and Commit Semantics

**Files:**
- Modify: `src-tauri/src/services/proxy/upstream.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/contract_tests.rs`

- [ ] **Step 1: Write streaming RED tests**

```rust
#[tokio::test]
async fn stream_bootstrap_fails_over_before_first_chunk() {
    let fixture = StreamFixture::new([
        stream_disconnect_before_data("a"),
        stream_chunks("b", [b"data: ok\n\n"]),
    ]);
    let response = fixture.engine.execute(streaming_chat_request()).await.unwrap();
    assert_eq!(response.selected_station_key_id(), Some("b"));
    assert_eq!(response.fallback_count(), 1);
}

#[tokio::test]
async fn committed_stream_error_never_selects_another_candidate() {
    let fixture = StreamFixture::new([
        stream_then_error("a", b"data: first\n\n"),
        stream_chunks("b", [b"data: forbidden\n\n"]),
    ]);
    let mut response = fixture.engine.execute(streaming_chat_request()).await.unwrap();
    assert_eq!(response.next_chunk().await.unwrap(), b"data: first\n\n"[..]);
    assert!(response.next_chunk().await.is_err());
    assert_eq!(fixture.attempted_ids(), ["a"]);
}
```

Add tests for first-byte timeout, 90-second idle timeout using paused Tokio time, client body drop, SSE usage/response ID capture, and capacity-guard release.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml stream_bootstrap_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml committed_stream_ -- --nocapture
```

Expected: FAIL because streams are not prefetched or committed.

- [ ] **Step 3: Prefetch the first non-empty chunk**

`UpstreamClientPool` returns response headers plus a byte stream. `ExecutionEngine` waits up to `upstream_first_byte_timeout` for the first non-empty chunk before constructing `ProxyExecutionResponse`. Failure in this phase is uncommitted and may follow RetryPolicy; the 180-second precommit deadline wraps all attempts.

- [ ] **Step 4: Stream through the finalizing body**

`FinalizingBody::stream` emits the prefetched chunk first, then polls the upstream stream under a reset-on-each-chunk idle timeout. It feeds `SseUsageObserver`, tracks first-token timing, and finalizes on EOF/error/drop. Once ingress receives a prepared stream response, no code path can call `ExecutionEngine` again for that request.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml stream_bootstrap_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml committed_stream_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml sse_observer_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_contract_never_fails_over_after_stream_output -- --nocapture
```

Expected: all pass; downstream drop is logged but does not penalize the candidate, upstream error does.

- [ ] **Step 6: Commit stream lifecycle**

```powershell
git add -- src-tauri/src/services/proxy/upstream.rs src-tauri/src/services/proxy/execution.rs src-tauri/src/services/proxy/response_body.rs src-tauri/src/services/proxy/observability.rs src-tauri/src/services/proxy/contract_tests.rs
git diff --cached --check
git commit -m "feat: enforce proxy stream commit semantics"
```

## Task 15: Migrate Chat, Responses, and the Streaming Chat Bridge

**Files:**
- Modify: `src-tauri/src/services/proxy/endpoint_adapter.rs`
- Create: `src-tauri/src/services/proxy/responses_chat_stream.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/contract_tests.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] **Step 1: Write Chat and direct Responses RED parity tests**

```rust
#[tokio::test]
async fn v2_chat_and_responses_streams_preserve_sse_and_usage() {
    for endpoint in [RouteEndpointKind::ChatCompletions, RouteEndpointKind::Responses] {
        let result = run_v2_stream_case(endpoint).await;
        assert!(result.chunks.len() > 1, "stream must not be buffered");
        assert_eq!(result.content_type, "text/event-stream");
        assert_eq!(result.request_log.first_token_ms.is_some(), true);
        assert_eq!(result.request_log.total_tokens, Some(12));
    }
}
```

- [ ] **Step 2: Write the Responses-to-Chat stream-transform RED test**

```rust
#[test]
fn chat_sse_decoder_emits_valid_responses_events_across_split_chunks() {
    let mut decoder = ResponsesChatStreamDecoder::new("gpt-test", "resp_test");
    let first = decoder.push(br#"data: {"choices":[{"delta":{"content":"Hel"}}]}\n"#).unwrap();
    assert!(first.is_empty());
    let second = decoder.push(br#"\ndata: {"choices":[{"delta":{"content":"lo"}}],"usage":{"prompt_tokens":5,"completion_tokens":1}}\n\ndata: [DONE]\n\n"#).unwrap();
    let text = String::from_utf8(second.concat()).unwrap();
    assert!(text.contains("response.created"));
    assert!(text.contains("response.output_text.delta"));
    assert!(text.contains("response.completed"));
    assert!(text.contains("Hello"));
}
```

Also cover tool-call deltas, finish reason, `[DONE]`, CRLF, malformed JSON, 256 KiB pending buffer, upstream error, and no duplicate completed event.

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_chat_and_responses_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml chat_sse_decoder_ -- --nocapture
```

Expected: FAIL because streaming adapter transforms are not wired.

- [ ] **Step 4: Implement direct stream paths**

Chat and Responses direct streams pass through safe upstream SSE bytes while feeding the existing observer. Ensure Responses sets `Accept: text/event-stream` if the client omitted it, and preserves explicit safe headers.

- [ ] **Step 5: Implement the sealed streaming bridge**

`responses_chat_stream.rs` is a stateful decoder owned by the Responses adapter. It converts Chat delta/tool/usage events to valid Responses events without buffering the entire response. It generates one response ID, emits created before the first delta, emits completed once, and reports malformed upstream SSE as `upstream_stream_failed`.

- [ ] **Step 6: Run GREEN and real loopback parity**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml v2_chat_and_responses_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml chat_sse_decoder_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml committed_stream_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_contract_ -- --nocapture
```

Expected: all pass; streaming responses arrive in multiple chunks and do not fall back to non-stream mode.

- [ ] **Step 7: Commit streaming endpoints**

```powershell
git add -- src-tauri/src/services/proxy/endpoint_adapter.rs src-tauri/src/services/proxy/responses_chat_stream.rs src-tauri/src/services/proxy/response_body.rs src-tauri/src/services/proxy/execution.rs src-tauri/src/services/proxy/contract_tests.rs src-tauri/src/services/proxy/mod.rs
git diff --cached --check
git commit -m "feat: migrate proxy streaming endpoints"
```

## Task 16: Complete Differential, Resource, and Security Verification

**Files:**
- Create: `src-tauri/src/services/proxy/soak_tests.rs`
- Modify: `src-tauri/src/services/proxy/contract_tests.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Create: `scripts/local-proxy-v2-boundary.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: Add the full differential matrix**

For each legacy/v2 case, compare status, public error code, selected key, fallback count, route reason, upstream path/body, feedback target, and final log. Include: Models, both Usage paths, Embeddings, Chat buffered/stream, Responses direct buffered/stream, Responses-to-Chat buffered/stream, aliases, tools, reasoning, cache fields, 400/401/403/404/408/425/429/500, connect failure, delayed first byte, stream reset, and update drain.

- [ ] **Step 2: Add deterministic resource-soak tests**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v2_soak_returns_all_resource_counters_to_zero() {
    let fixture = V2SoakFixture::start().await;
    fixture.run_short_requests(1000).await;
    fixture.run_stream_disconnects(100).await;
    fixture.wait_for_quiescence(Duration::from_secs(5)).await;
    assert_eq!(fixture.active_connections(), 0);
    assert_eq!(fixture.active_requests(), 0);
    assert_eq!(fixture.body_budget_used(), 0);
    assert_eq!(fixture.duplicate_finalizations(), 0);
}
```

Also assert the 65th raw connection is closed without a spawned connection task, while the 33rd parsed request receives `503 local_proxy_busy`; neither path may grow tasks or memory without bound.

- [ ] **Step 3: Add the production-boundary source contract**

`scripts/local-proxy-v2-boundary.test.mjs` must assert:

```js
assert.doesNotMatch(server, /std::net::TcpListener|thread::spawn|httparse|ureq/);
assert.doesNotMatch(execution, /TcpStream|httparse|ureq/);
assert.doesNotMatch(endpointAdapter, /record_station_key|insert_request_log/);
assert.match(runtime, /ProxyRuntimeMode/);
```

Register it in `run-contract-tests.mjs`.

- [ ] **Step 4: Run the complete automated gate**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml v2_soak_returns_all_resource_counters_to_zero -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all pass. If a pre-existing unrelated failure from Task 0 remains, prove the proxy-focused and staged-snapshot checks separately and record the blocker; do not call the migration complete.

- [ ] **Step 5: Run dependency and secret audits**

```powershell
cargo tree --manifest-path src-tauri/Cargo.toml -d
rg -n "sk-local-pool-change-me|authorization|api_key|cookie" output src-tauri/target -g '*.log' -g '*.json'
```

Expected: no duplicate Reqwest 0.12/0.13, and generated logs contain no real credential. The placeholder may exist only in migration/tests.

- [ ] **Step 6: Commit verification assets**

```powershell
git add -- src-tauri/src/services/proxy/soak_tests.rs src-tauri/src/services/proxy/contract_tests.rs src-tauri/src/services/proxy/mod.rs scripts/local-proxy-v2-boundary.test.mjs scripts/run-contract-tests.mjs
git diff --cached --check
git commit -m "test: gate v2 proxy reliability"
```

## Task 17: Switch the Default Runtime and Verify Live Clients

**Files:**
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `docs/PROJECT_PLAN.md`

- [ ] **Step 1: Write the default-selection RED test**

```rust
#[test]
fn production_runtime_defaults_to_v2_without_environment_override() {
    assert_eq!(ProxyRuntimeMode::production_default(), ProxyRuntimeMode::V2);
}
```

Run it and expect FAIL while Legacy is still the default.

- [ ] **Step 2: Switch production default to v2**

Keep `RELAY_POOL_PROXY_RUNTIME=legacy` available only in debug builds for one release cycle:

```rust
pub fn production_default() -> Self { Self::V2 }

fn parse_dev_override(value: Option<&str>) -> Option<Self> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "legacy" => Some(Self::Legacy),
        "v2" => Some(Self::V2),
        _ => None,
    }
}

#[cfg(debug_assertions)]
fn dev_override() -> Option<Self> {
    let value = std::env::var("RELAY_POOL_PROXY_RUNTIME").ok();
    Self::parse_dev_override(value.as_deref())
}
```

Add a pure parser test covering `legacy`, `v2`, mixed case, whitespace, missing values, and invalid values; do not mutate the process environment from parallel tests. Release builds must ignore the environment override. Update `docs/PROJECT_PLAN.md` to describe the mature HTTP stack and limited endpoint scope without presenting the temporary selector as a permanent user feature.

- [ ] **Step 3: Run the automated gate again**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all pass with v2 as the default.

- [ ] **Step 4: Start the actual Tauri app**

Use an external target directory so file watching does not rebuild recursively:

```powershell
$env:CARGO_TARGET_DIR = (Resolve-Path 'output').Path + '\tauri-dev-target'
pnpm.cmd tauri:dev
```

Verify `relay-pool-desktop.exe`, WebView2 children, `127.0.0.1:1430`, and the configured local proxy port.

- [ ] **Step 5: Run live acceptance**

Using a fresh test data directory:

1. Import Relay Pool into CC-Switch and verify the stored provider key is not the placeholder.
2. Through CC-Switch call `/v1/models`, Chat, Responses stream, and Embeddings.
3. Through Codex run one reasoning + tool-call Responses stream.
4. Cause candidate A to fail before output and verify candidate B, log, and health agree.
5. Cause a committed stream to break and verify no candidate B request occurs.
6. Trigger update drain during a stream and verify status/counters converge.
7. Inspect logs/SQLite for no key, authorization, cookie, or request body.

Record request IDs and sanitized outcomes in `output/local-routing-v2-acceptance/`; never commit the output directory.

- [ ] **Step 6: Commit the default switch**

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs docs/PROJECT_PLAN.md
git diff --cached --check
git commit -m "feat: enable v2 local routing runtime"
```

## Task 18: Remove the Legacy Runtime and Close the Migration

**Release precondition:** Execute this as a separate release task, not in the Task 17 branch or release. Start only after one build with v2 as the production default has been shipped and the Task 17 live matrix has been rerun against that shipped build with no rollback, duplicate-finalization, hang, credential leak, or committed-stream failover defect. Until that evidence exists, stop after Task 17 and keep the debug-only legacy selector.

**Files:**
- Delete: `src-tauri/src/services/proxy/legacy_runtime.rs`
- Delete: `src-tauri/src/services/proxy/http_request.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `scripts/local-proxy-auth-contract.test.mjs`
- Modify: `scripts/request-cost-model-pricing.test.mjs`
- Modify: `scripts/local-proxy-v2-boundary.test.mjs`

- [ ] **Step 1: Prove the release precondition**

Run:

```powershell
git tag --contains (git log -1 --format=%H -- src-tauri/src/services/proxy/runtime.rs)
Get-ChildItem -LiteralPath output/local-routing-v2-acceptance -File
```

Expected: at least one shipped release tag contains the Task 17 default-switch commit, and sanitized acceptance evidence exists for that shipped build. If either condition is absent, do not delete legacy code.

- [ ] **Step 2: Strengthen the deletion contract and run RED**

Add these assertions before deleting files:

```js
await assert.rejects(access("src-tauri/src/services/proxy/legacy_runtime.rs"));
await assert.rejects(access("src-tauri/src/services/proxy/http_request.rs"));
assert.doesNotMatch(cargoToml, /^httparse\s*=/m);
assert.doesNotMatch(proxySources, /std::net::TcpListener|thread::spawn|ureq::/);
```

Run `node scripts/local-proxy-v2-boundary.test.mjs`; expect FAIL while legacy exists.

- [ ] **Step 3: Delete the legacy implementation and selector**

Remove `ProxyRuntimeMode`, debug override, delegation branches, raw HTTP writer/parser, socket fixtures tied only to legacy, and temporary differential helpers. Keep long-lived v2 contract and soak tests.

- [ ] **Step 4: Remove proxy-only dependencies**

Remove `httparse` from Cargo. Keep `ureq` because collectors/updater/channel monitors still use it, but prove no production file under `services/proxy/` references it. Do not expand this task into a repo-wide Ureq migration.

- [ ] **Step 5: Update source contracts to target final modules**

Move auth assertions to `ingress.rs`, cost assertions to `execution.rs`/`routing_repository.rs`, and v2 boundary assertions to final files. Remove all references to `legacy_runtime.rs`.

- [ ] **Step 6: Run the final verification gate**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml prepare_for_update -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
rg -n "httparse|std::net::TcpListener|thread::spawn|ureq::" src-tauri/src/services/proxy
```

Expected: all test/build commands exit 0; final `rg` exits 1 with no matches.

- [ ] **Step 7: Verify the staged snapshot**

```powershell
git add -- src-tauri/src/services/proxy/legacy_runtime.rs src-tauri/src/services/proxy/http_request.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock scripts/local-proxy-auth-contract.test.mjs scripts/request-cost-model-pricing.test.mjs scripts/local-proxy-v2-boundary.test.mjs
git diff --cached --name-only
git diff --cached --check
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: only final migration files are staged; Cargo check passes against the staged-equivalent working tree.

- [ ] **Step 8: Commit legacy removal**

```powershell
git commit -m "refactor: retire legacy local proxy runtime"
```

## Final Release Gate

- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture`.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml prepare_for_update -- --nocapture`.
- [ ] Run `pnpm.cmd test:contracts`.
- [ ] Run `pnpm.cmd test`.
- [ ] Run `pnpm.cmd build`.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- [ ] Run the Task 17 live CC-Switch and Codex matrix against the final, legacy-free runtime.
- [ ] Prove `git status --short` contains only intentionally preserved user changes and ignored/untracked build outputs.
- [ ] Prove `git log --oneline --max-count=20` shows each task as an independently reviewable commit.

The migration is complete only when the final source has one HTTP runtime, one execution loop, one outcome finalizer, bounded resources, no production proxy parser/socket thread code, and fresh live evidence for first-run import plus streaming.
