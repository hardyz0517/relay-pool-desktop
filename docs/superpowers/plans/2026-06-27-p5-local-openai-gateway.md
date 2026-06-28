# P5 Local OpenAI-Compatible Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Relay Pool Desktop into a local OpenAI-compatible gateway so external tools connect to one localhost endpoint while Key Pool selection, protocol adaptation, fallback, request logging, and key health feedback happen inside the app.

**Architecture:** Treat P5 as a gateway, not a single endpoint forwarder. The gateway core owns `127.0.0.1` lifecycle and the OpenAI-compatible surface area; the router chooses `Station Key` candidates from Key Pool; adapters translate client requests into the owning station's upstream protocol; the management surface keeps Station, Key Pool, Logs, Settings, and Routing readable. Keep routing and protocol conversion separate so new upstream formats can be added without rewriting the local server.

**Tech Stack:** Tauri 2, Rust, SQLite, existing React + TypeScript + Vite UI, existing Key Pool / request log models, OpenAI-compatible JSON, SSE for streaming, local-only loopback HTTP server.

---

## What Already Exists

P5.0-P5.4 core work is now shipped and must be preserved:

- local HTTP server on `127.0.0.1`
- `start / stop / restart / status`
- `GET /v1/models` aggregation and deduplication
- `POST /v1/chat/completions` non-streaming
- `POST /v1/chat/completions` SSE passthrough
- `POST /v1/responses` non-streaming and SSE passthrough
- CORS / OPTIONS compatibility
- enabled + priority routing across Station Keys
- request logs stored in SQLite
- settings / dashboard / logs / key pool / channel status UI wired to proxy state

This plan records the P5 gateway baseline and the remaining post-P5 work. P5 has expanded the product model from "local forwarder" to "local gateway".

## Reference Notes

Use these public references as behavioral inspiration only, not as copy targets:

- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) for local gateway framing, management API, and multi-protocol endpoint thinking
- [CC Switch](https://github.com/farion1231/cc-switch) for provider management UX and OpenAI-compatible / Responses compatibility direction

The plan intentionally follows the same broad product shape:

- external tools only talk to one local endpoint
- the local app selects a routable key
- client protocol and upstream protocol are separate concerns
- metadata-only logs and readable errors matter

## File Map

### Core gateway files

- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Create or modify: `src-tauri/src/services/proxy/http.rs`
- Create or modify: `src-tauri/src/services/proxy/router.rs`
- Create or modify: `src-tauri/src/services/proxy/adapters/mod.rs`
- Create or modify: `src-tauri/src/services/proxy/adapters/openai.rs`
- Create or modify: `src-tauri/src/services/proxy/adapters/responses.rs`
- Create or modify: `src-tauri/src/services/proxy/logging.rs`
- Create or modify: `src-tauri/src/services/proxy/stream.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Add or modify: `src-tauri/src/models/proxy.rs`

### Model and routing files

- Modify: `src-tauri/src/models/stations.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/models/collector.rs`
- Modify: `src-tauri/src/models/capture.rs`

### Frontend files

- Modify: `src/lib/api/proxy.ts`
- Modify: `src/lib/types/proxy.ts`
- Modify: `src/lib/api/stations.ts`
- Modify: `src/lib/types/stations.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/app/routes.tsx`
- Modify: `src/components/shell/AppShell.tsx`

### Docs

- Modify: `README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PHASE_5_LOCAL_PROXY_PLAN.md`
- Modify: `docs/superpowers/specs/2026-06-27-p5-gateway-design.md`
- Modify: `docs/superpowers/plans/2026-06-27-p5-local-openai-gateway.md`

---

## P5 Phase Map

### P5.0

Already done:

- local HTTP server
- only `127.0.0.1`
- start / stop / restart / status
- `/v1/models`
- `/v1/chat/completions` non-streaming
- explicit bounded behavior for `stream: true`
- Key Pool enabled + priority routing
- request logs

### P5.1

Done in the current P5 line: first-class `POST /v1/responses` support and a clear client-vs-upstream protocol split.

### P5.2

Done in the current P5 line: explicit SSE passthrough for chat completions and responses with strict fallback boundaries.

### P5.3

Done in the current P5 line at MVP depth: request results feed Key Pool usage metadata and a request-log-derived Channel Status view.

### P5.4

Done in the current P5 line at MVP depth: CORS / OPTIONS, model-list aggregation/dedup, URL normalization guardrails, error shape, and safety hardening. Later work can add model mapping, timeout controls, and deeper health scoring.

---

## Task 1: Lock the Gateway Contract

**Files:**
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Add: `src-tauri/tests/proxy_gateway_contract.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write the failing contract tests**

Add tests that document the current gateway boundary and the behavior the rest of P5 must preserve:

```rust
#[test]
fn models_returns_openai_style_list() { /* object=list, stable ids */ }

#[test]
fn chat_completions_non_streaming_returns_choice() { /* choices[0].message.content exists */ }

#[test]
fn chat_completions_stream_true_passes_through_sse() { /* text/event-stream passthrough */ }

#[test]
fn no_enabled_keys_returns_readable_503() { /* no enabled key path is clear */ }

#[test]
fn request_log_redacts_sensitive_metadata() { /* no prompt, response, or full key */ }
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml proxy_gateway_contract -- --nocapture
```

Expected: tests fail or are incomplete before implementation, but they define the contract.

- [ ] **Step 2: Split transport from request handling**

Keep `runtime.rs` as lifecycle and accept-loop only. Move request parsing, routing, and response rendering into a smaller HTTP module so future `Responses` and streaming work do not keep expanding the runtime file.

Minimum shape:

```rust
// runtime.rs
pub fn start(...)
pub fn stop(...)
pub fn status(...)

// http.rs
pub fn handle_connection(...)
pub fn handle_proxy_request(...)
```

- [ ] **Step 3: Re-run the contract tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml proxy_gateway_contract -- --nocapture
```

Expected: the tests still fail where behavior is missing, but the file split should not change current behavior.

## Task 2: Make `Responses` a First-Class Endpoint

**Files:**
- Create: `src-tauri/src/services/proxy/adapters/responses.rs`
- Create: `src-tauri/src/services/proxy/adapters/openai.rs`
- Create: `src-tauri/src/services/proxy/http.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing tests for `POST /v1/responses`**

Add tests that assert:

- `POST /v1/responses` is accepted
- the request is normalized into an internal response-shaped request
- the result can be rendered back into an OpenAI-style response
- `chat/completions` stays supported for older tools

Key assertions:

```rust
assert_eq!(response.status_code, 200);
assert_eq!(parsed["object"], "response");
assert!(parsed["id"].as_str().is_some());
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml responses -- --nocapture
```

- [ ] **Step 2: Add an internal protocol enum**

Introduce a small protocol model that distinguishes:

```txt
client_request_kind:
- chat_completions
- responses

upstream_api_format:
- auto
- openai_chat_completions
- openai_responses
- custom_openai_compatible
```

Keep this enum small and serializable only where needed. Do not turn it into a broad policy engine.

- [ ] **Step 3: Implement the minimal `Responses` router**

Implement:

- body parsing
- model extraction
- messages or input normalization
- upstream request shape selection
- response rendering back to the client

Keep the first version intentionally small:

- no complex tool calling
- no multi-modal conversion
- no broad object reconstruction beyond what the MVP needs

- [ ] **Step 4: Re-run the focused tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml responses -- --nocapture
```

Expected: `POST /v1/responses` passes and `POST /v1/chat/completions` still passes.

## Task 3: Separate Client Protocol from Upstream Protocol

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/stations.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`

- [ ] **Step 1: Write tests for protocol selection**

Add tests covering:

- station-level upstream format defaults to `auto`
- a station can explicitly declare `openai_chat_completions`
- a station can explicitly declare `openai_responses`
- the gateway chooses the correct upstream shape from the selected station

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml protocol -- --nocapture
```

- [ ] **Step 2: Add the minimal schema fields**

Add a small upstream-format field where the routing decision belongs. Prefer station-level configuration unless future adapter work proves a key-level override is needed.

Keep the first version simple:

```txt
stations.upstream_api_format
stations.upstream_api_base_path
```

Only add more fields if tests prove they are needed.

- [ ] **Step 3: Surface the format in the UI**

Expose the upstream format in station editing and key management only as much as needed to explain why a given upstream is treated as chat-only, responses-capable, or custom-compatible.

- [ ] **Step 4: Verify the P5.0 route does not regress**

Run:

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: the current local proxy behavior remains intact.

## Task 4: Add Streaming Support Without Breaking Fallback Rules

**Files:**
- Create: `src-tauri/src/services/proxy/stream.rs`
- Modify: `src-tauri/src/services/proxy/http.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing streaming tests**

Add tests for:

- `stream:true` on chat completions
- `stream:true` on responses
- stream starts only after upstream is chosen
- once streaming has begun, fallback does not silently switch keys mid-stream

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml stream -- --nocapture
```

- [ ] **Step 2: Implement SSE framing in one place**

Keep SSE logic isolated in `stream.rs` so the non-streaming router stays simple.

Expected responsibilities:

- write OpenAI-style SSE chunks
- write `[DONE]`
- emit readable stream errors
- close the stream cleanly on upstream failure

- [ ] **Step 3: Keep fallback rules strict**

Streaming fallback rules:

- before the first chunk: key fallback is allowed
- after the first chunk: do not switch keys silently
- on mid-stream failure: return a stream error and log the failure

- [ ] **Step 4: Re-run the streaming tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml stream -- --nocapture
```

Expected: streaming behavior is explicit and does not break the earlier contract tests.

## Task 5: Add Request Log and Key State Feedback

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src/lib/api/proxy.ts`
- Modify: `src/lib/types/proxy.ts`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Write tests for log metadata and key feedback**

Add tests asserting:

- request logs store path, model, status, fallback_count, station_key_id, station_id
- request logs do not store prompt text or full response bodies
- key rows can show last_used_at, last_failure_at, failure_count, success_count, last_error

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log -- --nocapture
```

- [ ] **Step 2: Add the minimum key feedback fields**

Prefer small, useful fields:

```txt
last_used_at
last_success_at
last_failure_at
success_count
failure_count
last_error_summary
```

Keep any health score derived from these fields rather than introducing another write-heavy state.

- [ ] **Step 3: Pipe logs into the UI**

Update the logs page to show:

- path
- model
- key
- station
- status
- fallback count
- error summary

The detailed inspector should remain metadata-only.

- [ ] **Step 4: Re-run the proxy and UI smoke**

Run:

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Then manually verify the logs page reads real request rows.

## Task 6: Expand the Management Surface Around the Gateway

**Files:**
- Modify: `src/app/routes.tsx`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`

- [ ] **Step 1: Write UI behavior tests or focused UI smoke steps**

Document and verify:

- settings page can start / stop / restart proxy
- dashboard shows local gateway status
- key pool shows recent usage and failure state
- logs page shows real request metadata
- channel status no longer reads like a pure mock surface

- [ ] **Step 2: Keep the UI wording aligned with the gateway model**

The copy should consistently say:

- Station = account asset
- Station Key = routable object
- Key Pool = routing pool
- Router = key selection and fallback
- Gateway = local localhost endpoint

- [ ] **Step 3: Add only the controls the operator needs**

Do not overbuild this stage. Add:

- proxy port
- start / stop / restart
- response to proxy state
- routing strategy display
- log clear / refresh

Avoid turning settings into a policy editor.

- [ ] **Step 4: Re-run the front-end build**

Run:

```powershell
pnpm build
```

Expected: the UI still compiles cleanly with the new gateway surfaces.

## Task 7: Add Compatibility, Error Normalization, and Safety Hardening

**Files:**
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/http.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src/lib/api/proxy.ts`
- Modify: `src/lib/types/proxy.ts`
- Modify: `README.md`
- Modify: `docs/PROJECT_PLAN.md`

- [ ] **Step 1: Add compatibility tests**

Cover:

- CORS / OPTIONS behavior
- OpenAI-compatible error envelopes
- `/v1/models` dedup / stable ordering
- upstream URL normalization
- timeout behavior
- unsupported request error clarity

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml compatibility -- --nocapture
```

- [ ] **Step 2: Normalize all error surfaces**

Standardize:

- response shape
- upstream failure text
- fallback failure text
- unsupported feature text
- no-enabled-key text

Everything should remain readable from a client like Chatbox, VS Code, or Codex.

- [ ] **Step 3: Tighten secret handling**

Verify:

- no prompt / response body in logs
- no full key in logs
- no session / cookie / token in logs
- upstream URLs are redacted if they might leak sensitive details

- [ ] **Step 4: Re-run the manual smoke chain**

Run:

```powershell
curl.exe http://127.0.0.1:<port>/v1/models
curl.exe http://127.0.0.1:<port>/v1/chat/completions
```

Expected: readable OpenAI-style responses and errors.

## Task 8: Finish P5 with Documentation and Release Gates

**Files:**
- Modify: `README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PHASE_5_LOCAL_PROXY_PLAN.md`
- Modify: `docs/superpowers/specs/2026-06-27-p5-gateway-design.md`
- Modify: `docs/superpowers/plans/2026-06-27-p5-local-openai-gateway.md`

- [ ] **Step 1: Update the P5 docs to match the gateway framing**

Make sure the docs say:

- P5 is a local gateway
- `Station Key` is the routing object
- `Responses` is first-class
- fallback belongs to the router
- streaming is not the same as normal forwarding

- [ ] **Step 2: Write the final smoke checklist**

Document the release gate:

```txt
1. Settings can start / stop / restart proxy
2. Proxy listens only on 127.0.0.1
3. /v1/models works
4. Non-streaming /v1/chat/completions works
5. stream:true returns SSE passthrough for chat completions and responses when the upstream supports it
6. Disabled keys are not used
7. Fallback works across retryable upstream failures
8. Request logs appear
9. Logs remain metadata-only
10. Stopping proxy makes the port unreachable
```

- [ ] **Step 3: Run the full verification chain**

Run:

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml -- --nocapture
```

Then run manual smoke against the real app:

- start proxy
- hit `/v1/models`
- hit `/v1/chat/completions`
- test `stream:true`
- confirm logs
- stop proxy
- confirm port closes

- [ ] **Step 4: Commit the final P5 cut**

Create one final commit only after the above gates pass and the user-facing docs match the actual behavior.

---

## P5 Success Definition

P5 is complete when:

- Relay Pool Desktop is a local OpenAI-compatible gateway, not just a chat forwarder.
- External tools can connect to one localhost endpoint.
- Key Pool routing, fallback, and logs are all real.
- `Responses` is supported as the forward path for modern clients.
- Streaming works explicitly or is clearly and safely bounded.
- The UI, docs, and runtime behavior all describe the same product model.
