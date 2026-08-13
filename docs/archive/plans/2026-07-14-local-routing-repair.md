# Local Routing Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the localhost gateway authenticated, protocol-faithful, cache-observable, and scheduler-correct for real Codex-style Responses traffic without replacing the existing Tauri proxy architecture.

**Architecture:** Keep socket ownership and request lifecycle in `runtime.rs`, pure scheduling in `scheduler/`, persistence in `database.rs`, and React behind typed query/API modules. Deliver the repair as four independently reviewable workstreams: security and protocol fidelity, scheduler correctness, stream/cache observability, and status/endpoint completeness. Each workstream must be releasable and rollback-safe on its own.

**Tech Stack:** Rust 2021, Tauri 2, rusqlite, serde_json, ureq, React 18, TypeScript, TanStack Query, Vite, Node source-contract scripts, Cargo unit/integration tests.

---

## Detailed Repair Report

### Confirmed Current State

- The desktop debug process is running from the current checkout, but the local proxy port `8787` is not listening.
- The local database has two enabled, schedulable candidates inside the active GPT group and multiplier ceiling. Both have normal balance and no active cooldown.
- Both candidates currently advertise `supports_tools = false` and `supports_reasoning = false`. A normal Codex request containing tools or a `reasoning` object is rejected before any upstream call.
- The local request log currently contains monitor probes only. It cannot establish a real user-traffic cache hit rate.
- Focused proxy tests pass (`142/142`) and the frontend production build passes, but five checked-in Node source-contract scripts are stale and the full Rust suite has two unrelated failures.

### Confirmed Defects

| Priority | Defect | Root cause | Required outcome |
|---|---|---|---|
| P0 | Local access key is not enforced | Proxy dispatch never reads or validates `settings.local_key` | Every non-OPTIONS request requires the configured Bearer token |
| P0 | Fresh installs use a public placeholder local key | Database seed stores `sk-local-pool-change-me` verbatim | Generate an OS-random key and rotate only the known placeholder value |
| P0 | Cross-origin localhost access is open | Responses emit `Access-Control-Allow-Origin: *` | Only loopback browser origins receive CORS permission |
| P0 | Responses-to-Chat fallback loses semantics and cache controls | Adapter rebuilds only `model`, `stream`, and `messages` | Preserve compatible fields; explicitly reject state that cannot be represented |
| P0 | Explicit Chat-format stations receive an unconverted Responses body | `upstream_responses_path()` changes the path without normalizing the body | Normalize before the first Chat-format request |
| P1 | Streaming Responses never bind response ID affinity | Response ID is extracted only from buffered JSON | Capture IDs from SSE and bind after successful completion |
| P1 | Model 404 stops cross-key fallback | 404 is request-scoped and marked non-retryable | Try the next eligible key without damaging key health |
| P1 | Scheduler wait settings are inert | Runtime only calls `try_acquire()` and never waits | Implement bounded sticky/fallback queues with timeout and cancellation cleanup |
| P1 | `schedulable` is missing from rich/read candidates | Queries omit the column and conversions hardcode `true` | Apply the field in initial selection, revalidation, legacy policies, and status |
| P1 | Capability editing is inconsistent | One editor defaults false while another forces every capability true | Use one shared default and always load/persist actual capabilities |
| P1 | Upstream stream failures do not affect runtime quality metrics | Write outcome does not distinguish upstream read from downstream disconnect | Penalize upstream interruption only; never penalize client disconnect |
| P2 | Cache writes for current models display as zero | Observer does not read `cache_write_tokens` | Normalize current and legacy cache-write fields into request logs |
| P2 | Routing status is polluted by monitor traffic | Snapshot reads the newest unfiltered request log | Use proxy-origin logs only for route decisions/events |
| P2 | Status preview is not request-realistic | It is fixed to Chat, no model/tools/reasoning, and zero capacity | Label it as baseline eligibility and expose actual decision facts separately |
| P2 | HTTP compatibility is incomplete | Query strings are discarded, chunked bodies unsupported, headers over-filtered | Preserve request target, decode bounded chunked bodies, forward safe headers |
| P2 | Embeddings is modeled but not served | Dispatcher has no `/v1/embeddings` branch | Add non-streaming embeddings routing and fallback |

### Cache-Specific Decision

Native Chat Completions and native Responses requests should remain byte-equivalent except for intentional model alias rewriting and Authorization replacement. The repair must not parse and rebuild a native request merely to inspect it.

For Responses-to-Chat fallback, exact semantic preservation is impossible for server-side Responses state. The adapter must therefore:

1. preserve `prompt_cache_key`, `prompt_cache_options`, `prompt_cache_retention`, compatible message content, tools, tool choice, sampling fields, and token limits;
2. translate `instructions` into the first developer message;
3. reject `previous_response_id`, `conversation`, built-in Responses tools, and streaming Chat fallback with an explicit OpenAI-shaped error instead of silently deleting them;
4. keep native Responses streaming unchanged;
5. record `cached_tokens` and `cache_write_tokens` from buffered and SSE results.

This plan intentionally does not build a complete Chat-SSE-to-Responses-SSE protocol emulator. That would be a separate feature with a larger compatibility matrix.

## Delivery Boundaries

Implement as four pull-request-sized workstreams, in this order:

1. **Security and protocol fidelity:** Tasks 1-4.
2. **Scheduler correctness:** Tasks 5-7.
3. **Stream, cache, and status truth:** Tasks 8-10.
4. **HTTP and endpoint completeness:** Tasks 11-12.

Do not mix unrelated Change Center work currently present in the checkout. Never use `git add .`, `git add -A`, or `git commit -a`.

## File Structure

Create:

- `src-tauri/src/services/proxy/local_auth.rs` - Bearer parsing, constant-time comparison, loopback-origin policy.
- `src-tauri/src/services/proxy/http_request.rs` - bounded HTTP request-line/header/body parsing and chunked decoding.
- `src-tauri/src/services/proxy/responses_chat_fallback.rs` - strict Responses-to-Chat conversion and typed incompatibility errors.
- `src/features/key-pool/stationKeyCapabilityDefaults.ts` - one frontend capability-default contract.
- `scripts/local-proxy-auth-contract.test.mjs` - frontend/local-key API contract guard.
- `scripts/station-key-capability-defaults.test.mjs` - shared default and no-hardcode guard.

Modify:

- `src-tauri/Cargo.toml` - add `httparse` for structured header parsing.
- `src-tauri/src/services/proxy/mod.rs` - export focused modules; make candidate schedulability explicit.
- `src-tauri/src/services/proxy/runtime.rs` - authentication, forwarding, fallback, stream completion, Embeddings, safe headers.
- `src-tauri/src/services/proxy/adapters/responses.rs` - keep native helpers; delegate Chat fallback conversion.
- `src-tauri/src/services/proxy/observability.rs` - response ID and cache-write observation.
- `src-tauri/src/services/proxy/routing_failure.rs` - retryable request-scoped 404.
- `src-tauri/src/services/proxy/router.rs` - preserve schedulable facts in rich candidates.
- `src-tauri/src/services/proxy/routing_snapshot.rs` - filtered logs and truthful baseline preview.
- `src-tauri/src/services/proxy/routing_types.rs` - baseline-preview fields.
- `src-tauri/src/services/proxy/scheduler/capacity.rs` - bounded wait acquisition.
- `src-tauri/src/services/database.rs` - candidate schedulability and proxy-only request-log query.
- `src/features/key-pool/EditKeyPage.tsx` - stop forcing all capabilities true.
- `src/features/key-pool/KeyPoolPage.tsx` - shared defaults and schedulable control.
- `src/lib/types/localRouting.ts` - truthful baseline-preview naming.
- `src/lib/types/proxy.ts` - preserve the existing cache-write compatibility field.
- `src/features/routing/LocalRoutingCandidateRow.tsx` - schedulable and baseline eligibility display.
- `src/features/routing/LocalRoutingStatusCandidateRow.tsx` - schedulable status display.
- `src/features/routing/LocalRoutingStatusTab.tsx` - baseline eligibility heading and actual decision summary.
- `scripts/dashboard-local-route-start.test.mjs`
- `scripts/local-routing-automatic-settings.test.mjs`
- `scripts/local-routing-reorder.test.mjs`
- `scripts/request-log-observability-table.test.mjs`
- `scripts/sidebar-local-proxy-status.test.mjs`
- `docs/PROJECT_PLAN.md` - document supported endpoints and explicit fallback limits.

---

### Task 1: Repair the Existing Verification Baseline

**Files:**
- Modify: `scripts/dashboard-local-route-start.test.mjs`
- Modify: `scripts/local-routing-automatic-settings.test.mjs`
- Modify: `scripts/local-routing-reorder.test.mjs`
- Modify: `scripts/request-log-observability-table.test.mjs`
- Modify: `scripts/sidebar-local-proxy-status.test.mjs`

- [ ] **Step 1: Update the dashboard and shell assertions to the query-service architecture**

Replace direct `getProxyStatus`/local-state expectations with the current query contracts:

```js
assert.match(dashboardSource, /proxyStatusQueryOptions\(false\)/);
assert.match(dashboardSource, /requestLogsQueryOptions\(proxyStatusQuery\.data\?\.running \? 2_000 : false\)/);
assert.match(dashboardSource, /const proxyStatus = proxyStatusQuery\.data \?\? null/);

assert.match(appShellSource, /proxyStatusQueryOptions\(2_000\)/);
assert.match(appShellSource, /queryClient\.setQueryData\(queryKeys\.proxyStatus/);
assert.match(appShellSource, /PROXY_STATUS_UPDATED_EVENT/);
```

- [ ] **Step 2: Update candidate-row assertions without weakening drag behavior**

Use the actual drag handle contract and the current rejection field:

```js
assert.match(candidateRow, /previewRejectReasons/);
assert.match(candidateRowSource, /const isSortable = Boolean\(/);
assert.match(candidateRowSource, /\.\.\.dragAttributes/);
assert.match(candidateRowSource, /\.\.\.dragListeners/);
assert.match(candidateRowSource, /disabled=\{dragDisabled\}/);
```

- [ ] **Step 3: Update the request-log dashboard guard**

Assert the current dashboard query and recent-usage rendering instead of obsolete title text:

```js
assert.ok(
  pageSource.includes("<RequestLogTable") &&
    dashboardSource.includes("requestLogsQueryOptions") &&
    dashboardSource.includes("requestLogs.slice(0, 5)"),
  "logs and dashboard should consume the shared request-log query without sharing presentation",
);
```

- [ ] **Step 4: Run the repaired script gate**

Run:

```powershell
$tests = @(
  "scripts/dashboard-local-route-start.test.mjs",
  "scripts/local-routing-automatic-settings.test.mjs",
  "scripts/local-routing-reorder.test.mjs",
  "scripts/request-log-observability-table.test.mjs",
  "scripts/sidebar-local-proxy-status.test.mjs"
)
foreach ($test in $tests) { node $test; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } }
```

Expected: all five scripts exit `0`. If an assertion still targets removed architecture, replace it with an assertion on the current typed query/event boundary; do not delete behavior assertions merely to make the script pass.

- [ ] **Step 5: Commit the baseline repair**

```powershell
git add -- scripts/dashboard-local-route-start.test.mjs scripts/local-routing-automatic-settings.test.mjs scripts/local-routing-reorder.test.mjs scripts/request-log-observability-table.test.mjs scripts/sidebar-local-proxy-status.test.mjs
git diff --cached --name-only
git commit -m "test: repair local routing contract baseline"
```

Expected staged paths: exactly the five scripts above.

---

### Task 2: Enforce the Local Access Key and Restrict Browser Origins

**Files:**
- Create: `src-tauri/src/services/proxy/local_auth.rs`
- Create: `scripts/local-proxy-auth-contract.test.mjs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing Rust authentication tests**

Create `scripts/local-proxy-auth-contract.test.mjs` first so the source boundary also fails before implementation:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const proxyModule = await readFile("src-tauri/src/services/proxy/mod.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const localAuth = await readFile("src-tauri/src/services/proxy/local_auth.rs", "utf8").catch(() => "");
const database = await readFile("src-tauri/src/services/database.rs", "utf8");

assert.match(proxyModule, /mod local_auth;/);
assert.match(localAuth, /pub fn authorize_headers/);
assert.match(localAuth, /pub fn allowed_origin/);
assert.match(runtime, /database\.ensure_secure_local_access_key\(\)/);
assert.match(runtime, /local_auth::authorize_headers\(&request\.headers, &local_key\)/);
assert.match(runtime, /invalid_local_api_key/);
assert.match(runtime, /local_auth::allowed_origin/);
assert.doesNotMatch(runtime, /access-control-allow-origin:\s*\*/i);
assert.match(database, /ensure_secure_local_access_key/);
assert.match(database, /OsRng\.fill_bytes/);

console.log("local proxy authentication contract passed");
```

Add these tests to the bottom of the new `local_auth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn bearer_auth_requires_exact_configured_key() {
        let headers = HashMap::from([(
            "authorization".to_string(),
            "Bearer relay-local-secret".to_string(),
        )]);
        assert!(authorize_headers(&headers, "relay-local-secret"));
        assert!(!authorize_headers(&headers, "relay-local-other"));
        assert!(!authorize_headers(&HashMap::new(), "relay-local-secret"));
    }

    #[test]
    fn cors_allows_loopback_origins_only() {
        assert_eq!(allowed_origin("http://127.0.0.1:3000"), Some("http://127.0.0.1:3000"));
        assert_eq!(allowed_origin("http://localhost:5173"), Some("http://localhost:5173"));
        assert_eq!(allowed_origin("https://attacker.example"), None);
    }
}
```

Add this database test beside the settings tests:

```rust
#[test]
fn known_placeholder_local_key_is_rotated_once() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let first = database
        .ensure_secure_local_access_key()
        .expect("secure local key");
    let second = database
        .ensure_secure_local_access_key()
        .expect("stable local key");

    assert_ne!(first, "sk-local-pool-change-me");
    assert!(first.starts_with("sk-local-"));
    assert!(first.len() >= 50);
    assert_eq!(second, first);
}
```

- [ ] **Step 2: Run the RED test**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml local_auth --lib
node scripts/local-proxy-auth-contract.test.mjs
```

Expected: Rust fails because `local_auth` is not exported; Node fails because the module/source guards are absent.

- [ ] **Step 3: Implement exact Bearer validation and loopback-origin policy**

Create `local_auth.rs`:

```rust
use std::collections::HashMap;

pub fn authorize_headers(headers: &HashMap<String, String>, configured_key: &str) -> bool {
    let Some(value) = headers.get("authorization") else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")) else {
        return false;
    };
    constant_time_eq(token.trim().as_bytes(), configured_key.trim().as_bytes())
}

pub fn allowed_origin(origin: &str) -> Option<&str> {
    let origin = origin.trim();
    let loopback = origin == "http://localhost"
        || origin.starts_with("http://localhost:")
        || origin == "http://127.0.0.1"
        || origin.starts_with("http://127.0.0.1:");
    loopback.then_some(origin)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}
```

In `database.rs`, import `base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _}` plus `rand::{rngs::OsRng, RngCore}` and add:

```rust
const INSECURE_LOCAL_KEY_PLACEHOLDER: &str = "sk-local-pool-change-me";

pub fn ensure_secure_local_access_key(&self) -> Result<String, String> {
    let connection = self.connection()?;
    let current = read_setting(&connection, "local_key")?;
    if !current.trim().is_empty() && current != INSECURE_LOCAL_KEY_PLACEHOLDER {
        return Ok(current);
    }

    let mut random = [0_u8; 32];
    OsRng.fill_bytes(&mut random);
    let generated = format!("sk-local-{}", URL_SAFE_NO_PAD.encode(random));
    upsert_setting(&connection, "local_key", &generated)?;
    Ok(generated)
}
```

Use `ensure_secure_local_access_key()` instead of `get_local_access_key()` in the request authentication path. This rotates only the checked-in placeholder or an empty value; it never changes a user-provided key.

In `handle_proxy_request`, permit `OPTIONS` only when the optional Origin is loopback, then authenticate every other route before dispatch:

```rust
if request.method == "OPTIONS" {
    return match request.headers.get("origin") {
        Some(origin) if local_auth::allowed_origin(origin).is_none() => {
            ProxyResponse::json_error(403, "cors_origin_denied", "Origin is not allowed")
        }
        _ => cors_preflight_response(),
    };
}
let local_key = match context.database.ensure_secure_local_access_key() {
    Ok(key) => key,
    Err(error) => return ProxyResponse::json_error(500, "local_auth_unavailable", &error),
};
if !local_auth::authorize_headers(&request.headers, &local_key) {
    return ProxyResponse::json_error(401, "invalid_local_api_key", "Invalid local API key");
}
```

In `handle_connection`, calculate the optional response origin from the parsed request before dispatch and pass it to `write_http_response`:

```rust
let cors_origin = request
    .headers
    .get("origin")
    .and_then(|origin| local_auth::allowed_origin(origin))
    .map(str::to_owned);
let response = handle_proxy_request(context, &request);
let write_result = write_http_response(
    &mut stream,
    response,
    started,
    cors_origin.as_deref(),
);
```

Change `write_http_response` to accept `cors_origin: Option<&str>`. Build the header with this exact branch and use it for buffered and streaming responses:

```rust
let cors_headers = cors_origin
    .map(|origin| {
        format!(
            "access-control-allow-origin: {origin}\r\n\
             access-control-allow-methods: GET, POST, OPTIONS\r\n\
             access-control-allow-headers: authorization, content-type, accept\r\n\
             vary: Origin\r\n"
        )
    })
    .unwrap_or_default();
```

Never reflect an arbitrary Origin and never emit `Access-Control-Allow-Origin: *`.

- [ ] **Step 4: Add an end-to-end loopback test**

Add these tests beside the existing `handle_proxy_request_returns_cors_preflight_response` test. The `/v1/usage` endpoint is deliberate: it proves authentication before dispatch without contacting an upstream.

```rust
#[test]
fn local_proxy_rejects_missing_or_wrong_local_key() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    database
        .update_local_access_key("relay-local-secret".to_string())
        .expect("local key");
    let context = proxy_context(database);
    let mut request = ParsedRequest {
        method: "GET".to_string(),
        path: "/v1/usage".to_string(),
        headers: HashMap::new(),
        body: Vec::new(),
    };

    assert_eq!(handle_proxy_request(&context, &request).status_code, 401);
    request.headers.insert(
        "authorization".to_string(),
        "Bearer wrong-secret".to_string(),
    );
    assert_eq!(handle_proxy_request(&context, &request).status_code, 401);
    request.headers.insert(
        "authorization".to_string(),
        "Bearer relay-local-secret".to_string(),
    );
    assert_eq!(handle_proxy_request(&context, &request).status_code, 200);
}

#[test]
fn proxy_cors_rejects_non_loopback_origin() {
    let context = proxy_context(AppDatabase::new_in_memory_for_tests().expect("database"));
    let request = ParsedRequest {
        method: "OPTIONS".to_string(),
        path: "/v1/responses".to_string(),
        headers: HashMap::from([(
            "origin".to_string(),
            "https://attacker.example".to_string(),
        )]),
        body: Vec::new(),
    };

    assert_eq!(handle_proxy_request(&context, &request).status_code, 403);
}
```

Replace the wildcard assertion in `write_http_response_includes_cors_compatibility_headers` with two TCP cases: call `write_http_response(..., Some("http://127.0.0.1:3000"))` and assert that exact origin plus `vary: Origin`; call it with `None` and assert the serialized response contains no `access-control-allow-origin` header.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml local_proxy_rejects_missing_or_wrong_local_key --lib
cargo test --manifest-path .\src-tauri\Cargo.toml proxy_cors_rejects_non_loopback_origin --lib
```

Expected: both PASS.

- [ ] **Step 5: Commit the authentication boundary**

```powershell
git add -- src-tauri/src/services/proxy/local_auth.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/database.rs scripts/local-proxy-auth-contract.test.mjs
git diff --cached --name-only
git commit -m "fix: enforce local proxy access key"
```

Deployment note: installations still using the published placeholder key will receive a one-time random key. Re-run the existing CCSwitch import action once after upgrade; custom keys remain unchanged.

---

### Task 3: Make Responses-to-Chat Fallback Strict and Cache-Preserving

**Files:**
- Create: `src-tauri/src/services/proxy/responses_chat_fallback.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/adapters/responses.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write failing adapter tests for preserved fields and rejected state**

```rust
#[test]
fn responses_chat_fallback_preserves_cache_and_tool_fields() {
    let body = json!({
        "model": "gpt-5.6",
        "instructions": "Use the repository rules.",
        "input": "Inspect the current change.",
        "tools": [{
            "type": "function",
            "name": "read_file",
            "description": "Read a file",
            "parameters": {"type": "object", "properties": {}}
        }],
        "tool_choice": "auto",
        "prompt_cache_key": "workspace-a",
        "prompt_cache_options": {"mode": "implicit"},
        "max_output_tokens": 512,
        "reasoning": {"effort": "high"}
    });

    let chat = normalize_for_chat(&body).expect("compatible fallback");
    assert_eq!(chat["prompt_cache_key"], "workspace-a");
    assert_eq!(chat["prompt_cache_options"]["mode"], "implicit");
    assert_eq!(chat["max_completion_tokens"], 512);
    assert_eq!(chat["reasoning_effort"], "high");
    assert_eq!(chat["messages"][0]["role"], "developer");
    assert_eq!(chat["tools"][0]["function"]["name"], "read_file");
}

#[test]
fn responses_chat_fallback_rejects_server_side_continuation_and_streaming() {
    assert_eq!(
        normalize_for_chat(&json!({"model":"gpt-5.6","input":"x","previous_response_id":"resp_1"}))
            .unwrap_err(),
        ResponsesChatFallbackError::PreviousResponseUnsupported,
    );
    assert_eq!(
        normalize_for_chat(&json!({"model":"gpt-5.6","input":"x","stream":true}))
            .unwrap_err(),
        ResponsesChatFallbackError::StreamingUnsupported,
    );
}
```

- [ ] **Step 2: Run the RED tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml responses_chat_fallback --lib
```

Expected: FAIL because the module, error enum, and `normalize_for_chat` do not exist.

- [ ] **Step 3: Implement the strict converter**

Use this public contract:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsesChatFallbackError {
    PreviousResponseUnsupported,
    ConversationUnsupported,
    StreamingUnsupported,
    BuiltInToolUnsupported(String),
    InvalidInput,
}

pub fn normalize_for_chat(body: &Value) -> Result<Value, ResponsesChatFallbackError> {
    if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        return Err(ResponsesChatFallbackError::StreamingUnsupported);
    }
    if body.get("previous_response_id").is_some() {
        return Err(ResponsesChatFallbackError::PreviousResponseUnsupported);
    }
    if body.get("conversation").is_some() {
        return Err(ResponsesChatFallbackError::ConversationUnsupported);
    }

    let mut output = serde_json::Map::new();
    copy(body, &mut output, "model", "model");
    copy(body, &mut output, "temperature", "temperature");
    copy(body, &mut output, "top_p", "top_p");
    copy(body, &mut output, "tool_choice", "tool_choice");
    copy(body, &mut output, "parallel_tool_calls", "parallel_tool_calls");
    copy(body, &mut output, "prompt_cache_key", "prompt_cache_key");
    copy(body, &mut output, "prompt_cache_options", "prompt_cache_options");
    copy(body, &mut output, "prompt_cache_retention", "prompt_cache_retention");
    copy(body, &mut output, "max_output_tokens", "max_completion_tokens");

    if let Some(effort) = body.pointer("/reasoning/effort").cloned() {
        output.insert("reasoning_effort".to_string(), effort);
    }
    output.insert("messages".to_string(), build_messages(body)?);
    if let Some(tools) = body.get("tools") {
        output.insert("tools".to_string(), convert_function_tools(tools)?);
    }
    Ok(Value::Object(output))
}
```

`build_messages()` must prepend `instructions` as a `developer` message and then preserve the order of compatible message/input blocks. `convert_function_tools()` must accept only `type == "function"` and wrap `name`, `description`, `parameters`, and optional `strict` inside Chat Completions' `function` object.

- [ ] **Step 4: Normalize before the first request for explicit Chat-format stations**

In `forward_responses_to_candidate`:

```rust
let explicit_chat = matches!(
    candidate.upstream_api_format,
    UpstreamApiFormat::OpenAiChatCompletions
);
if explicit_chat {
    let normalized = normalize_for_chat(body_value)
        .map_err(|error| responses_fallback_error_message(&error))?;
    let chat_body = serde_json::to_vec(&normalized)
        .map_err(|error| format!("serialize chat fallback request failed: {error}"))?;
    let response = forward_to_candidate_with_body(
        request,
        candidate,
        "/v1/chat/completions",
        &chat_body,
        false,
    )?;
    return Ok(render_responses_proxy_response(response, fallback_model));
}
```

Apply the same converter only after a `404 | 405 | 501` from `Auto` or `CustomOpenAiCompatible`. Convert incompatibility errors into a local OpenAI-shaped `400 responses_chat_fallback_incompatible`, not an upstream-health failure.

- [ ] **Step 5: Run focused and full proxy tests, then commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml responses_chat_fallback --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/src/services/proxy/responses_chat_fallback.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/adapters/responses.rs src-tauri/src/services/proxy/runtime.rs
git diff --cached --name-only
git commit -m "fix: preserve responses fallback semantics"
```

Expected: focused tests and all proxy tests PASS.

---

### Task 4: Preserve Request Target and Safe OpenAI Headers

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/services/proxy/http_request.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write failing parser and forwarding tests**

```rust
#[test]
fn request_parser_preserves_query_and_decodes_chunked_body() {
    let raw = b"POST /v1/responses?trace=1 HTTP/1.1\r\n\
Host: localhost\r\nTransfer-Encoding: chunked\r\n\r\n\
4\r\ntest\r\n0\r\n\r\n";
    let parsed = parse_http_request(&mut raw.as_slice(), 2 * 1024 * 1024).expect("request");
    assert_eq!(parsed.path, "/v1/responses");
    assert_eq!(parsed.target, "/v1/responses?trace=1");
    assert_eq!(parsed.body, b"test");
}

#[test]
fn forwarding_keeps_safe_openai_headers_and_replaces_authorization() {
    let headers = forwarded_headers(&HashMap::from([
        ("authorization".into(), "Bearer client-key".into()),
        ("openai-organization".into(), "org_1".into()),
        ("openai-project".into(), "proj_1".into()),
        ("idempotency-key".into(), "idem_1".into()),
        ("connection".into(), "keep-alive".into()),
    ]));
    assert!(!headers.contains_key("authorization"));
    assert!(!headers.contains_key("connection"));
    assert_eq!(headers["openai-organization"], "org_1");
    assert_eq!(headers["openai-project"], "proj_1");
    assert_eq!(headers["idempotency-key"], "idem_1");
}
```

- [ ] **Step 2: Run RED and add structured header parsing**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_parser_preserves_query --lib
```

Expected: FAIL.

Add:

```toml
httparse = "1"
```

Define:

```rust
pub struct ParsedRequest {
    pub method: String,
    pub path: String,
    pub target: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
```

Use `httparse::Request` for request-line and header parsing. Enforce a 64 KiB header limit and a 2 MiB body limit. Decode chunk sizes as hexadecimal, reject chunk extensions larger than the header limit, and stop only after the zero-size chunk plus trailer terminator.

- [ ] **Step 3: Forward the raw target and a strict safe-header allowlist**

Use `request.target` for native forwarding. Keep only:

```rust
const FORWARDED_REQUEST_HEADERS: &[&str] = &[
    "accept",
    "content-type",
    "idempotency-key",
    "openai-organization",
    "openai-project",
    "openai-beta",
    "user-agent",
];
```

Always replace Authorization with the Station Key. Never forward `host`, `connection`, `content-length`, `transfer-encoding`, cookies, proxy headers, or client-supplied forwarding headers.

- [ ] **Step 4: Preserve safe upstream response metadata**

Add `headers: Vec<(String, String)>` to `ProxyResponse` and preserve:

```rust
const FORWARDED_RESPONSE_HEADERS: &[&str] = &[
    "retry-after",
    "x-request-id",
    "openai-processing-ms",
    "x-ratelimit-limit-requests",
    "x-ratelimit-remaining-requests",
    "x-ratelimit-reset-requests",
    "x-ratelimit-limit-tokens",
    "x-ratelimit-remaining-tokens",
    "x-ratelimit-reset-tokens",
];
```

Generate local `content-length`, `connection`, and CORS headers separately so upstream cannot override them.

- [ ] **Step 5: Verify and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml http_request --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/services/proxy/http_request.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs
git diff --cached --name-only
git commit -m "fix: preserve local proxy HTTP semantics"
```

---

### Task 5: Apply `schedulable` in Every Candidate Path

**Files:**
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/routing_policy.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/routing_types.rs`
- Modify: `src/lib/types/localRouting.ts`
- Modify: `src/features/routing/LocalRoutingCandidateRow.tsx`
- Modify: `src/features/routing/LocalRoutingStatusCandidateRow.tsx`

- [ ] **Step 1: Write failing database and selector tests**

```rust
#[test]
fn rich_route_candidates_preserve_unschedulable_state() {
    let database = seeded_database();
    let key = create_key(&database, true, false);
    let candidates = database.proxy_rich_route_candidates().expect("candidates");
    let candidate = candidates.iter().find(|item| item.candidate.station_key_id == key.id)
        .expect("candidate remains explainable");
    assert!(!candidate.candidate.schedulable);
}

#[test]
fn every_policy_rejects_unschedulable_candidate() {
    let mut candidate = rich_candidate("key-a", 0, capabilities(|_| {}));
    candidate.candidate.schedulable = false;
    for policy in [RoutingPolicy::AutomaticBalanced, RoutingPolicy::PriorityFallback] {
        let request = route_request(RouteEndpointKind::Responses, Some("gpt-5.6"), false, policy);
        let selected = select_route_candidates(&request, vec![candidate.clone()], &[]).unwrap();
        assert!(selected.accepted.is_empty());
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml schedulable_candidate --lib
```

Expected: FAIL because `RouteCandidate` lacks `schedulable` and rich/read conversion hardcodes `true`.

- [ ] **Step 3: Carry the persisted field into the domain candidate**

Insert the field immediately after `key_enabled` in `RouteCandidate`:

```rust
pub schedulable: bool,
```

Select `k.schedulable` in both `proxy_route_candidates_from_connection_with_data_key` and `proxy_rich_route_candidates_from_connection_with_data_key`, adjust row indices once, and assign the persisted value. In `rich_candidate_to_scheduler_candidate` use:

```rust
schedulable: candidate.candidate.schedulable,
```

In legacy hard gates, reject when `!candidate.candidate.schedulable` before ranking.

- [ ] **Step 4: Make the routing read model truthful**

Add `schedulable: bool` to both Rust read structs and `schedulable: boolean` to the TypeScript row. Construct it from the persisted candidate:

```rust
let preview_reject_reasons = if candidate.schedulable {
    preview_reject_reasons
} else {
    let mut reasons = preview_reject_reasons;
    reasons.insert(0, "asset_unavailable".to_string());
    reasons
};
let preview_eligible = candidate.schedulable && preview_reject_reasons.is_empty();

// Add these three assignments in the LocalRoutingCandidateRow initializer.
schedulable: candidate.schedulable,
preview_eligible,
preview_reject_reasons,
```

In both candidate-row components, render the existing disabled/status treatment whenever `candidate.schedulable` is false and label it `已暂停路由`. Never report an unschedulable key as participating.

- [ ] **Step 5: Verify and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml schedulable --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_policy.rs src-tauri/src/services/proxy/routing_snapshot.rs src-tauri/src/services/proxy/routing_types.rs src-tauri/src/services/database.rs src/lib/types/localRouting.ts src/features/routing/LocalRoutingCandidateRow.tsx src/features/routing/LocalRoutingStatusCandidateRow.tsx
git diff --cached --name-only
git commit -m "fix: honor station key schedulability"
```

---

### Task 6: Unify Capability Defaults and Expose Schedulability

**Files:**
- Create: `src/features/key-pool/stationKeyCapabilityDefaults.ts`
- Create: `scripts/station-key-capability-defaults.test.mjs`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/lib/types/stationKeys.ts`

- [ ] **Step 1: Write the failing frontend contract**

```js
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const defaults = readFileSync("src/features/key-pool/stationKeyCapabilityDefaults.ts", "utf8");
const editPage = readFileSync("src/features/key-pool/EditKeyPage.tsx", "utf8");
const poolPage = readFileSync("src/features/key-pool/KeyPoolPage.tsx", "utf8");

assert.match(defaults, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS/);
assert.match(poolPage, /schedulable/);
assert.doesNotMatch(editPage, /supportsTools:\s*true/);
assert.doesNotMatch(editPage, /supportsReasoning:\s*true/);
assert.match(editPage, /getStationKeyCapabilities/);
```

- [ ] **Step 2: Run RED**

```powershell
node scripts/station-key-capability-defaults.test.mjs
```

Expected: FAIL because the shared module and schedulable form control do not exist and `EditKeyPage` forces booleans.

- [ ] **Step 3: Add one conservative OpenAI-compatible default**

```ts
import type { StationKeyCapabilities } from "@/lib/types/stationKeys";

export const OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS = {
  supportsChatCompletions: true,
  supportsResponses: true,
  supportsEmbeddings: false,
  supportsStream: true,
  supportsTools: false,
  supportsVision: false,
  supportsReasoning: false,
  modelAllowlist: [],
  modelBlocklist: [],
  preferredModels: [],
  onlyUseAsBackup: false,
  routingTags: [],
} satisfies Omit<StationKeyCapabilities, "stationKeyId" | "updatedAt">;
```

Conservative defaults remain false for unverified advanced capabilities. The repair is consistency, not optimistic inference.

- [ ] **Step 4: Load actual values before editing and persist schedulable separately**

`EditKeyPage` must initialize its form from `getStationKeyCapabilities(stationKeyId)` and save those values, never constants. `KeyPoolPage` must add:

```tsx
<CheckField
  label="参与自动路由"
  checked={form.schedulable}
  onChange={(schedulable) => onFormChange({ ...form, schedulable })}
/>
```

Add `schedulable` to `KeyPoolEditForm`, initialize it from `item.schedulable`, and pass it through create/update inputs. When capability loading fails, disable Save and show the existing error toast; do not leave a writable default form.

- [ ] **Step 5: Verify and commit**

```powershell
node scripts/station-key-capability-defaults.test.mjs
pnpm.cmd build
git add -- src/features/key-pool/stationKeyCapabilityDefaults.ts src/features/key-pool/EditKeyPage.tsx src/features/key-pool/KeyPoolPage.tsx src/lib/types/stationKeys.ts scripts/station-key-capability-defaults.test.mjs
git diff --cached --name-only
git commit -m "fix: unify station key routing capabilities"
```

Deployment note: existing keys with tools/reasoning disabled remain disabled. After release, the operator must explicitly enable verified capabilities for the two active keys; do not migrate user choices silently.

---

### Task 7: Implement Bounded Capacity Waiting

**Files:**
- Modify: `src-tauri/src/services/proxy/scheduler/capacity.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write failing capacity tests**

```rust
#[test]
fn bounded_wait_acquires_after_release_and_cleans_waiter() {
    let registry = Arc::new(CapacityRegistry::default());
    let first = registry.try_acquire("key-a", 1);
    let waiter_registry = Arc::clone(&registry);
    let waiter = std::thread::spawn(move || {
        waiter_registry.wait_acquire("key-a", 1, 1, Duration::from_secs(1))
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    while registry.waiting("key-a") != 1 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(registry.waiting("key-a"), 1);
    drop(first);
    assert!(matches!(waiter.join().unwrap(), CapacityWaitResult::Acquired(_)));
    assert_eq!(registry.waiting("key-a"), 0);
}

#[test]
fn bounded_wait_reports_queue_full_and_timeout() {
    let registry = Arc::new(CapacityRegistry::default());
    let _active = registry.try_acquire("key-a", 1);
    let admitted = registry.try_enter_wait("key-a", 1);
    assert!(matches!(
        registry.wait_acquire("key-a", 1, 1, Duration::from_millis(5)),
        CapacityWaitResult::QueueFull
    ));
    drop(admitted);
    assert!(matches!(
        registry.wait_acquire("key-a", 1, 1, Duration::from_millis(5)),
        CapacityWaitResult::TimedOut
    ));
    assert_eq!(registry.waiting("key-a"), 0);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml bounded_wait --lib
```

Expected: FAIL because `wait_acquire` and `CapacityWaitResult` do not exist.

- [ ] **Step 3: Implement a Condvar-backed registry**

Preserve the existing `try_acquire`, `try_enter_wait`, `CapacityGuard`, and `WaitingPermit` API. Replace their internal `Arc<Mutex<...>>` with one shared state/notification object so there is still exactly one `in_flight` and `waiting` counter per key:

```rust
use std::time::{Duration, Instant};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug)]
pub enum CapacityWaitResult {
    Acquired(CapacityGuard),
    QueueFull,
    TimedOut,
}

#[derive(Debug, Default)]
struct CapacityShared {
    states: Mutex<HashMap<String, CapacityState>>,
    changed: Condvar,
}

#[derive(Debug, Default)]
pub struct CapacityRegistry {
    shared: Arc<CapacityShared>,
}
```

Update `CapacityGuard` and `WaitingPermit` to hold `Option<Arc<CapacityShared>>`. Their existing `released` flag remains the single protection against double decrement. Implement the blocking operation exactly once in `CapacityRegistry`:

```rust
pub fn wait_acquire(
    &self,
    key: impl Into<String>,
    max_concurrency: i64,
    max_waiting: u64,
    timeout: Duration,
) -> CapacityWaitResult {
    let key = key.into();
    let deadline = Instant::now() + timeout;
    let mut states = self.shared.states.lock().expect("capacity registry poisoned");
    let capacity = states.entry(key.clone()).or_default();
    if max_waiting == 0 || capacity.waiting >= max_waiting {
        return CapacityWaitResult::QueueFull;
    }
    capacity.waiting += 1;

    loop {
        let capacity = states.entry(key.clone()).or_default();
        if max_concurrency <= 0 || capacity.in_flight < max_concurrency as u64 {
            capacity.waiting = capacity.waiting.saturating_sub(1);
            capacity.in_flight += 1;
            drop(states);
            return CapacityWaitResult::Acquired(CapacityGuard::new_acquired(
                Arc::clone(&self.shared),
                key,
            ));
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            capacity.waiting = capacity.waiting.saturating_sub(1);
            return CapacityWaitResult::TimedOut;
        }
        let (next_states, _) = self
            .shared
            .changed
            .wait_timeout(states, remaining)
            .expect("capacity registry poisoned");
        states = next_states;
    }
}
```

In `CapacityGuard::release()`, decrement under `shared.states`, drop the lock, then call `shared.changed.notify_all()`. `Drop` continues calling `release()`. This guarantees a streamed response wakes a waiter only when its response-owned guard is released.

- [ ] **Step 4: Execute bounded waiting after all immediate candidates are full**

Do not add a second scheduler-side queue or a parallel wait-decision type. In both `forward_automatic_chat_request` and `forward_automatic_responses_request`, retain the first capacity-full candidate as `wait_candidate`, continue trying the remaining eligible candidates immediately, and only wait when rescheduling returns no candidate:

```rust
let (max_waiting, timeout_seconds) = if route_request.previous_response_id.is_some()
    || route_request.session_hash.is_some()
{
    (
        advanced.sticky_max_waiting,
        advanced.sticky_wait_timeout_seconds,
    )
} else {
    (
        advanced.fallback_max_waiting,
        advanced.fallback_wait_timeout_seconds,
    )
};

match context.scheduler.wait_acquire(
    &wait_candidate.station_key_id,
    wait_candidate.max_concurrency,
    max_waiting,
    Duration::from_secs(timeout_seconds),
) {
    CapacityWaitResult::Acquired(guard) => {
        // Revalidate the same candidate once more, then forward with this guard.
    }
    CapacityWaitResult::QueueFull => {
        return ProxyResponse::json_error(
            503,
            "routing_capacity_exhausted",
            "自动路由等待队列已满",
        );
    }
    CapacityWaitResult::TimedOut => {
        return ProxyResponse::json_error(
            503,
            "routing_wait_timeout",
            "自动路由等待并发槽位超时",
        );
    }
}
```

Factor this duplicated block into a private `wait_for_candidate_capacity()` runtime helper after the first chat path is GREEN, then reuse it from Responses. Record the elapsed wait in `route_reason`/economic metadata; do not increment `fallback_count` merely because the chosen key was temporarily full. If post-wait revalidation fails, drop the guard before rescheduling.

- [ ] **Step 5: Verify concurrency behavior and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml scheduler::capacity --lib
cargo test --manifest-path .\src-tauri\Cargo.toml automatic_runtime_waits_after_all_topk_slots_are_full --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/src/services/proxy/scheduler/capacity.rs src-tauri/src/services/proxy/runtime.rs
git diff --cached --name-only
git commit -m "fix: implement bounded routing capacity waits"
```

---

### Task 8: Preserve Streaming Response Affinity and Classify Interruptions

**Files:**
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/metrics.rs`

- [ ] **Step 1: Write failing SSE response-ID and interruption tests**

```rust
#[test]
fn sse_observer_captures_response_id_and_usage() {
    let mut observer = SseUsageObserver::default();
    observer.push(br#"data: {"type":"response.created","response":{"id":"resp_123"}}\n\n"#);
    observer.push(br#"data: {"type":"response.completed","response":{"id":"resp_123","usage":{"input_tokens":10,"output_tokens":2}}}\n\n"#);
    assert_eq!(observer.response_id(), Some("resp_123"));
    assert_eq!(observer.usage().and_then(|usage| usage.input_tokens), Some(10));
}

#[test]
fn upstream_stream_read_failure_penalizes_runtime_metrics_but_client_disconnect_does_not() {
    let scheduler = SchedulerRuntimeState::default();
    report_stream_failure(&scheduler, "key-a", StreamFailureSource::UpstreamRead);
    assert!(scheduler.metrics_snapshot("key-a").error_rate_ewma > 0.0);

    let clean = SchedulerRuntimeState::default();
    report_stream_failure(&clean, "key-a", StreamFailureSource::DownstreamWrite);
    assert_eq!(clean.metrics_snapshot("key-a").error_rate_ewma, 0.0);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sse_observer_captures_response_id --lib
cargo test --manifest-path .\src-tauri\Cargo.toml upstream_stream_read_failure --lib
```

Expected: FAIL.

- [ ] **Step 3: Extend stream observation and write outcome**

Add `response_id: Option<String>` to `SseUsageObserver` and capture `/response/id` without storing response content. Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamFailureSource {
    UpstreamRead,
    DownstreamWrite,
}

struct ResponseWriteOutcome {
    first_token_ms: Option<i64>,
    usage: Option<ObservedUsage>,
    response_id: Option<String>,
    error_message: Option<String>,
    failure_source: Option<StreamFailureSource>,
}

fn report_stream_failure(
    scheduler: &SchedulerRuntimeState,
    station_key_id: &str,
    source: StreamFailureSource,
) {
    if source == StreamFailureSource::UpstreamRead {
        scheduler.report_result(station_key_id, false, None);
    }
}
```

Keep the existing `error_message` field because request-log construction consumes it. When `body.read()` fails, set `failure_source = Some(UpstreamRead)`; when `stream.write_all()` fails, set `failure_source = Some(DownstreamWrite)`.

- [ ] **Step 4: Bind the observed response ID after stream completion**

Extend the existing helper signature and pass `None` from non-stream unit tests/call sites until they have an observed ID:

```diff
 fn commit_pending_successes(
     context: &ProxyServerContext,
     pending_successes: &[PendingCandidateSuccess],
     completed_at: Option<&str>,
     completed_duration_ms: Option<i64>,
     first_token_ms: Option<i64>,
+    observed_response_id: Option<&str>,
 ) {
```

Pass `write_result.response_id.as_deref()` from `handle_connection`. Prefer the observed ID over the pending buffered ID:

```rust
let response_id = observed_response_id.or(success.response_id.as_deref());
if let (Some(scope), Some(response_id)) = (success.routing_group_scope.as_deref(), response_id) {
    context.scheduler.bind_response(
        scope,
        response_id,
        &success.station_key_id,
        now_ms,
        advanced.sticky_response_ttl_seconds as i64,
    );
}
```

If `write_result.failure_source == Some(StreamFailureSource::UpstreamRead)`, call `report_stream_failure` for each pending candidate success but do not replay after output began. On `DownstreamWrite`, finalize the log as interrupted without penalizing the Station Key. Do not call `commit_pending_successes` on either failure path.

- [ ] **Step 5: Verify and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sse_observer --lib
cargo test --manifest-path .\src-tauri\Cargo.toml stream_write_failure --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/src/services/proxy/observability.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/scheduler/metrics.rs
git diff --cached --name-only
git commit -m "fix: preserve streaming route affinity"
```

---

### Task 9: Make Model 404 Retryable Without Damaging Health

**Files:**
- Modify: `src-tauri/src/services/proxy/routing_failure.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write the failing classifier and runtime tests**

```rust
#[test]
fn model_not_found_is_request_scoped_but_retryable_before_output() {
    let failure = classify_route_failure(RouteFailureInput::http_status(404, false));
    assert_eq!(failure.kind, RouteFailureKind::ModelUnavailable);
    assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
    assert!(failure.retryable_before_output);
}

#[test]
fn runtime_falls_back_after_first_key_returns_model_404() {
    let missing = test_upstream_status(404, "Not Found", &[]);
    let accepted = test_upstream_json_success_times("model-fallback", false, None, 2);
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key_a = create_test_station_key(&database, "model-missing", &missing.base_url);
    let key_b = create_test_station_key(&database, "model-fallback", &accepted.base_url);
    database
        .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
        .expect("reorder");
    let context = proxy_context(database);

    let response = forward_chat_request(
        &context,
        &chat_request("model-only-on-second", false),
    );
    let first_health = context
        .database
        .get_station_key_health(key_a.id.clone())
        .expect("first health");

    assert_eq!(response.status_code, 200);
    assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
    assert_eq!(response.fallback_count, 1);
    assert_eq!(first_health.failure_count, 0);
    assert_eq!(first_health.consecutive_failures, 0);

    missing.join();
    accepted.join();
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml model_not_found_is_request_scoped --lib
cargo test --manifest-path .\src-tauri\Cargo.toml runtime_falls_back_after_first_key_returns_model_404 --lib
```

Expected: FAIL because 404 is currently non-retryable.

- [ ] **Step 3: Allow request-scoped retry**

Change the request-only constructor:

```rust
fn request_only(kind: RouteFailureKind, retryable_before_output: bool) -> Self {
    Self {
        kind,
        action: RouteFailureAction::IgnoreForKeyHealth,
        scope: RouteFailureScope::RequestOnly,
        retryable_before_output,
        retry_after_ms: None,
    }
}
```

Use `request_only(ModelUnavailable, !input.output_started)` for 404 and `request_only(BadRequest, false)` for 400. Keep request-scoped failures out of persistent health and error-rate EWMA.

- [ ] **Step 4: Verify fallback and no-health-damage behavior**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml routing_failure --lib
cargo test --manifest-path .\src-tauri\Cargo.toml runtime_falls_back_after_first_key_returns_model_404 --lib
```

Expected: PASS, with first-key failure count unchanged.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_failure.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs
git diff --cached --name-only
git commit -m "fix: fall back after model not found"
```

---

### Task 10: Normalize Current Cache-Write Metrics and Filter Routing Logs

**Files:**
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src-tauri/src/services/proxy/routing_types.rs`
- Modify: `src/lib/types/localRouting.ts`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Modify: `scripts/request-log-observability-table.test.mjs`

- [ ] **Step 1: Write failing cache-write parsing tests**

```rust
#[test]
fn observed_usage_reads_current_cache_write_tokens() {
    let usage = ObservedUsage::from_json(&json!({
        "usage": {
            "input_tokens": 2006,
            "output_tokens": 300,
            "input_tokens_details": {
                "cached_tokens": 1920,
                "cache_write_tokens": 64
            }
        }
    })).expect("usage");
    assert_eq!(usage.cache_read_tokens, Some(1920));
    assert_eq!(usage.cache_creation_tokens, Some(64));
}
```

- [ ] **Step 2: Run RED and implement compatibility parsing**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml observed_usage_reads_current_cache_write_tokens --lib
```

Add current names before legacy names:

```rust
let cache_creation_tokens = integer(usage, &["cache_write_tokens", "cache_creation_tokens", "cache_creation_input_tokens"])
    .or_else(|| integer_at(usage, "/input_tokens_details/cache_write_tokens"))
    .or_else(|| integer_at(usage, "/prompt_tokens_details/cache_write_tokens"))
    .or_else(|| integer_at(usage, "/input_tokens_details/cache_creation_tokens"))
    .or_else(|| integer_at(usage, "/prompt_tokens_details/cache_creation_tokens"));
```

Keep the persisted/frontend field name `cache_creation_tokens` / `cacheCreationTokens` in this repair to avoid a schema migration; the UI label is already the generic “缓存写”.

- [ ] **Step 3: Add a proxy-only request-log query**

```rust
pub fn list_local_proxy_request_logs(&self) -> Result<Vec<RequestLog>, String> {
    let connection = self.connection()?;
    list_local_proxy_request_logs_from_connection(&connection)
}

fn list_local_proxy_request_logs_from_connection(
    connection: &Connection,
) -> Result<Vec<RequestLog>, String> {
    let sql = format!(
        "SELECT {REQUEST_LOG_SELECT_COLUMNS}
         FROM request_logs
         WHERE COALESCE(route_policy, '') != 'channel_monitor'
         ORDER BY created_at DESC
         LIMIT 500"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取本地路由日志列表失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_request_log)
        .map_err(|error| format!("查询本地路由日志失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析本地路由日志失败: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|log| request_log_with_estimated_cost(connection, log))
        .collect())
}
```

This SQL is static; do not accept a caller-provided predicate. Add the regression beside request-log database tests:

```rust
#[test]
fn list_local_proxy_request_logs_excludes_channel_monitor_rows() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    {
        let connection = database.connection().expect("connection");
        connection
            .execute(
                "INSERT INTO request_logs
                 (id, started_at, method, path, stream, status, route_policy, created_at)
                 VALUES (?1, ?2, 'POST', '/v1/responses', 0, 'success', ?3, ?2)",
                params!["monitor-row", "1000", "channel_monitor"],
            )
            .expect("monitor row");
        connection
            .execute(
                "INSERT INTO request_logs
                 (id, started_at, method, path, stream, status, route_policy, created_at)
                 VALUES (?1, ?2, 'POST', '/v1/responses', 0, 'success', ?3, ?2)",
                params!["proxy-row", "2000", "cost_stable_first"],
            )
            .expect("proxy row");
    }

    let logs = database
        .list_local_proxy_request_logs()
        .expect("local proxy logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].id, "proxy-row");
}
```

Change `load_local_routing_workspace()` to call `database.list_local_proxy_request_logs()?`; the resulting slice must be the single source for `latest_decision`, `last_decision_at`, and `recent_events`. The general Logs page continues using `list_request_logs()` and may show monitor rows.

- [ ] **Step 4: Rename preview semantics without breaking serialization**

Add an explicit serialized enum instead of an untyped string:

```rust
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRoutingPreviewKind {
    BaselineEligibility,
}

pub struct LocalRoutingSettingsView {
    pub enabled: bool,
    pub bind_addr: String,
    pub port: u16,
    pub endpoint: RouteEndpointKind,
    pub policy: String,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub fallback_enabled: bool,
    pub preview_kind: LocalRoutingPreviewKind,
}
```

Construct the settings value with `preview_kind: LocalRoutingPreviewKind::BaselineEligibility`. Mirror the camelCase field in the frontend contract:

```ts
export type LocalRoutingPreviewKind = "baseline_eligibility";

export type LocalRoutingSettings = {
  enabled: boolean;
  bindAddr: string;
  port: number;
  endpoint: RouteEndpointKind;
  policy: string;
  maxRateMultiplier: number | null;
  routingGroupFilter: RoutingGroupFilter;
  fallbackEnabled: boolean;
  previewKind: LocalRoutingPreviewKind;
};
```

In `LocalRoutingStatusTab.tsx`, replace the `候选顺序预览` heading with this typed label:

```tsx
const candidateHeading =
  workspace.settings.previewKind === "baseline_eligibility"
    ? "候选基础资格"
    : "候选资格";

<h3 className="text-sm font-semibold text-foreground">{candidateHeading}</h3>
```

Do not use copy that implies a baseline row is the live winner. Actual decisions remain sourced only from the filtered proxy logs.

- [ ] **Step 5: Verify and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml cache_write_tokens --lib
cargo test --manifest-path .\src-tauri\Cargo.toml local_routing_workspace_ignores_channel_monitor_logs --lib
node scripts/request-log-observability-table.test.mjs
pnpm.cmd build
git add -- src-tauri/src/services/proxy/observability.rs src-tauri/src/services/database.rs src-tauri/src/services/proxy/routing_snapshot.rs src-tauri/src/services/proxy/routing_types.rs src/lib/types/localRouting.ts src/features/routing/LocalRoutingStatusTab.tsx scripts/request-log-observability-table.test.mjs
git diff --cached --name-only
git commit -m "fix: report truthful routing and cache facts"
```

---

### Task 11: Add the Missing Embeddings Route

**Files:**
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `docs/PROJECT_PLAN.md`

- [ ] **Step 1: Write failing dispatch and fallback tests**

```rust
fn embeddings_request(model: &str) -> ParsedRequest {
    ParsedRequest {
        method: "POST".to_string(),
        path: "/v1/embeddings".to_string(),
        target: "/v1/embeddings".to_string(),
        headers: HashMap::from([(
            "content-type".to_string(),
            "application/json".to_string(),
        )]),
        body: serde_json::to_vec(&json!({
            "model": model,
            "input": "relay pool",
        }))
        .expect("body"),
    }
}

#[test]
fn embeddings_request_routes_only_to_embeddings_capable_key() {
    let skipped = test_upstream_json_success("non-embeddings", false);
    let accepted = test_upstream_json_success_times("embeddings", false, None, 2);
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key_a = create_test_station_key(&database, "non-embeddings", &skipped.base_url);
    let key_b = create_test_station_key(&database, "embeddings", &accepted.base_url);
    let mut capabilities = default_capabilities_input(key_b.id.clone());
    capabilities.supports_embeddings = true;
    database
        .update_station_key_capabilities(capabilities)
        .expect("embeddings capability");
    database
        .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
        .expect("reorder");

    let response = forward_embeddings_request(
        &proxy_context(database),
        &embeddings_request("text-embedding-3-small"),
    );

    assert_eq!(response.status_code, 200);
    assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
    assert!(!skipped.was_called());

    skipped.join();
    accepted.join();
}

#[test]
fn embeddings_request_falls_back_before_output() {
    let failed = test_upstream_status(500, "Server Error", &[]);
    let accepted = test_upstream_json_success_times("embeddings-fallback", false, None, 2);
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key_a = create_test_station_key(&database, "embeddings-failed", &failed.base_url);
    let key_b = create_test_station_key(&database, "embeddings-fallback", &accepted.base_url);
    for key in [&key_a, &key_b] {
        let mut capabilities = default_capabilities_input(key.id.clone());
        capabilities.supports_embeddings = true;
        database
            .update_station_key_capabilities(capabilities)
            .expect("embeddings capability");
    }
    database
        .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
        .expect("reorder");

    let response = forward_embeddings_request(
        &proxy_context(database),
        &embeddings_request("text-embedding-3-small"),
    );

    assert_eq!(response.status_code, 200);
    assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
    assert_eq!(response.fallback_count, 1);

    failed.join();
    accepted.join();
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml embeddings_request --lib
```

Expected: FAIL because dispatcher returns 404.

- [ ] **Step 3: Add a non-streaming route request**

Add dispatch:

```rust
("POST", "/v1/embeddings") => forward_embeddings_request(context, request),
```

Build:

```rust
fn route_request_for_embeddings(
    request: &ParsedRequest,
    model: Option<String>,
    body: &Value,
    settings: &AppSettings,
) -> RouteRequest {
    RouteRequest {
        endpoint: RouteEndpointKind::Embeddings,
        model,
        stream: false,
        uses_tools: false,
        uses_vision: uses_vision(body),
        uses_reasoning: false,
        policy: parse_routing_policy(&settings.default_routing_strategy),
        max_rate_multiplier: settings.max_rate_multiplier,
        routing_group_filter: settings.default_routing_group_filter.clone(),
        session_hash: request_session_hash(request, body),
        previous_response_id: None,
        excluded_key_ids: Vec::new(),
        current_station_key_id: None,
        allow_depleted_fallback: settings.allow_depleted_fallback,
        now_ms: now_millis_for_services() as i64,
    }
}
```

Generalize the buffered fallback helper to accept `RouteEndpointKind` and use `/v1/embeddings` unchanged.

- [ ] **Step 4: Document the supported endpoint boundary**

Update `docs/PROJECT_PLAN.md` to list:

- `/v1/models`
- `/v1/chat/completions`
- `/v1/responses`
- `/v1/embeddings`
- `/v1/usage`

State explicitly that Files, Batches, Audio, Images, Realtime, and Assistants are not routed by this local gateway release.

- [ ] **Step 5: Verify and commit**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml embeddings_request --lib
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
git add -- src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_snapshot.rs docs/PROJECT_PLAN.md
git diff --cached --name-only
git commit -m "feat: route embeddings through the local gateway"
```

---

### Task 12: Final Integration, Runtime Smoke Test, and Release Gate

**Files:**
- Modify only if verification exposes a defect: files already owned by Tasks 1-11
- Do not modify: unrelated Change Center files

- [ ] **Step 1: Run formatting and frontend contracts**

```powershell
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
$tests = Get-ChildItem scripts -File -Filter '*.test.mjs' | Where-Object { $_.Name -match 'routing|proxy|dashboard-local-route|sidebar-local-proxy|request-log|station-key-capability' } | Sort-Object Name
foreach ($test in $tests) { node $test.FullName; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } }
pnpm.cmd build
```

Expected: all selected scripts exit `0`; TypeScript/Vite build exits `0`.

- [ ] **Step 2: Run focused and full Rust verification**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml services::proxy:: --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: proxy tests have zero failures. The full suite must have zero failures before release; do not waive the two pre-existing failures. Repair or isolate those baseline failures in separate exact-path commits before publishing.

- [ ] **Step 3: Start the current-source desktop app and verify the process**

```powershell
pnpm.cmd tauri:dev
```

Verify the specific `src-tauri\target\debug\relay-pool-desktop.exe` process, the Vite listener on `127.0.0.1:1430`, and the local proxy listener on the configured port after clicking Start.

- [ ] **Step 4: Run a local mock-upstream smoke matrix**

Use loopback mock upstreams only; do not consume real provider quota for the release gate.

| Case | Expected |
|---|---|
| No local Bearer key | 401 `invalid_local_api_key` |
| Wrong local Bearer key | 401 `invalid_local_api_key` |
| Native Responses, buffered | Body/usage preserved |
| Native Responses, SSE | Response ID affinity bound after completion |
| Explicit Chat fallback with compatible body | OpenAI-shaped Responses result |
| Chat fallback with `previous_response_id` | 400 explicit incompatibility, no silent field loss |
| First key model 404, second key success | 200, fallback count 1, first key health unchanged |
| All keys full | bounded wait, then acquire or explicit timeout |
| Upstream stream read error | interrupted log and runtime error EWMA update |
| Downstream disconnect | interrupted log, no key-health penalty |
| Embeddings-capable second key | routes to second key |
| Attacker browser Origin | no permissive CORS header |

- [ ] **Step 5: Verify current real configuration without sending prompts**

Read the local routing workspace and confirm:

- at least one active key is explicitly marked for tools and reasoning before configuring Codex;
- active keys are schedulable, inside the chosen group, under the multiplier ceiling, not depleted, and not cooling;
- the routing status shows no monitor probe as the latest user route;
- the local access key used by CCSwitch matches the current stored key.

- [ ] **Step 6: Confirm exact repository scope**

```powershell
git status --short
git diff --cached --name-only
git log --oneline -12 --decorate
```

Expected: no staged paths; unrelated Change Center work remains untouched; implementation commits are visible in task order.

## Acceptance Checklist

- [ ] Local proxy rejects missing and wrong local keys.
- [ ] CORS never grants arbitrary web origins access to the local gateway.
- [ ] Native Chat and Responses bodies are not rebuilt unnecessarily.
- [ ] Chat fallback preserves cache fields and compatible tool/message semantics.
- [ ] Unsupported stateful/streaming fallback returns an explicit local error.
- [ ] Streaming Responses IDs create group-scoped affinity only after success.
- [ ] Prompt cache write/read usage is visible for current response shapes.
- [ ] 404 model failure tries the next key without marking the first key unhealthy.
- [ ] `schedulable = false` blocks every real and simulated route path.
- [ ] Capability forms load and save actual values consistently.
- [ ] Sticky/fallback queue limits and timeouts affect real runtime behavior.
- [ ] Upstream stream interruption and downstream disconnect have different health effects.
- [ ] Routing status excludes channel-monitor probes.
- [ ] Query strings, chunked request bodies, and safe OpenAI headers survive the proxy.
- [ ] Embeddings routes only through capable keys.
- [ ] Frontend build, focused proxy suite, and full Rust suite all pass.

## Explicit Non-Goals

- Do not implement a full Chat Completions SSE to Responses SSE emulator in this repair.
- Do not add Files, Batches, Audio, Images, Realtime, or Assistants routing.
- Do not infer tools/reasoning support from a model name alone.
- Do not change Station Key capability choices already stored by the user.
- Do not persist raw prompts, response bodies, session identifiers, Authorization headers, or API keys.
- Do not redesign the routing page outside the truthfulness changes specified above.
