# Phase 5 Local Proxy MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a local OpenAI-compatible proxy MVP that routes requests through the enabled Station Key pool and serves a stable localhost endpoint for external tools.

**Architecture:** Keep the existing Station / Station Key / Key Pool / Collector split. P5 adds a local Rust HTTP server owned by Tauri state, a proxy runtime state model, request logging, and a narrow routing layer that selects Station Key first and Station second. The implementation should stay local-only, non-streaming at first, and fallback-aware without turning into a full policy engine.

**Tech Stack:** Tauri 2, Rust, SQLite, existing station/key/collector models, existing request log patterns, existing React/Ts front end, local HTTP server in Rust.

---

## Background

P4 and P4.1 are complete:

- Station is a login-account asset.
- Station Key is the routeable API key.
- Key Pool is the global priority and fallback view.
- Collector focuses on login-state information collection.
- WebView capture is only an advanced fallback.

P5 starts the first real local OpenAI-compatible proxy. The goal is not a full policy engine. The goal is a small, reliable localhost endpoint that external tools can call, and that selects an enabled key, forwards the request, and logs what happened.

## P5 Goals

### User Flow

1. User opens Relay Pool Desktop.
2. User adds one or more Stations and Station Keys.
3. User enables at least one key in the Key Pool.
4. User starts the local proxy from Settings or Overview.
5. External tools call `http://127.0.0.1:<port>/v1/models` or `/v1/chat/completions`.
6. The proxy selects the first enabled Station Key by priority.
7. The proxy forwards to that key's owning station.
8. If the upstream fails in a retryable way, the proxy falls back to the next key.
9. The proxy returns an OpenAI-compatible response.
10. The request is written to request logs.

### MVP Results

P5 should produce:

- Local server lifecycle control.
- `GET /v1/models`.
- `POST /v1/chat/completions` without streaming first.
- Enabled-key selection by priority.
- Retryable fallback across keys.
- Request logs with real proxy metadata.
- UI indicators for proxy running / stopped / error states.

## Non-Goals

P5 does not implement:

- Complex price-optimal routing.
- Full strategy groups.
- Allowlist / blocklist model policy.
- Full SSE streaming.
- Usage billing.
- Complex health checks.
- New collector work.
- Further WebView capture work.
- Secret encryption migration.
- Public network exposure.

P5 listens only on `127.0.0.1`.

## Routing Object

P5 routes by `Station Key`, not by `Station`.

Routing flow:

```txt
list key pool
filter enabled
sort by priority
for each key:
  load owning station
  build upstream URL from station.base_url
  inject Authorization: Bearer <station_key.api_key>
  forward request
  if success return
  if retryable failure try next key
if all failed return readable error
```

The station remains the account container. The key is the executable routing unit.

## API Compatibility Scope

P5 MVP must support:

- `GET /v1/models`
- `POST /v1/chat/completions`

Optional later:

- `POST /v1/responses`
- `POST /v1/embeddings`

`/v1/chat/completions` starts as non-streaming only.

- If `stream: true` is requested, P5 may return a clear unsupported error.
- Do not pretend streaming is fully implemented.

## Local Server Route

Preferred approach:

- Rust in-process HTTP server.
- Lifecycle owned by Tauri state.
- Bind only to `127.0.0.1`.
- Port comes from Settings.
- Port conflict returns a readable error.
- App shutdown stops the server.
- Front end can query runtime status.

Alternative approaches to evaluate during implementation:

1. Rust embedded HTTP server
2. Tauri sidecar process
3. Frontend dev server reuse, which is not acceptable for production proxy

Preferred choice:

```txt
Rust embedded HTTP server, managed by Tauri state.
```

## Data Structures

### Proxy Runtime State

```txt
running
bind_addr
port
started_at
last_error
active_requests
```

### Request Log Row

```txt
id
started_at
finished_at
duration_ms
method
path
model
stream
status
station_key_id
station_id
upstream_base_url
fallback_count
error_message
created_at
```

Rules:

- Do not store full prompt text.
- Do not store full response bodies.
- Do not store full API keys.
- Keep metadata enough to debug routing.
- Error messages must be short and redacted.

## Tauri Commands

Recommended commands:

```txt
get_proxy_status
start_local_proxy
stop_local_proxy
restart_local_proxy
list_request_logs
clear_request_logs
```

The exact naming may be adjusted to match existing command patterns, but the responsibilities should stay the same.

## UI Updates

### Overview

Show:

- Local proxy running status.
- Bind address.
- Enabled key count.
- Today's request count.
- Recent errors.

### Settings

Show:

- Local proxy port.
- Start / stop / restart controls.
- Local-only bind note.
- Startup on boot placeholder.

### Key Pool

Keep the existing layout.

Add only small status fields if needed:

- Last used time.
- Last failure.
- Basic health state.

### Request Logs

Replace the mock log view with real `request_logs`.

Show:

- Time.
- Path.
- Model.
- Key used.
- Owning station.
- Status.
- Duration.
- Fallback count.
- Error summary.

### Routing Rules

P5 only shows the current default strategy:

```txt
按 Key 池全局优先级 fallback
```

Do not build a complex strategy editor.

## Fallback and Error Policy

Fallback should happen for:

- Network timeout.
- Connection failure.
- 5xx responses.
- 429 responses.
- Retryable upstream failures.

Fallback should not blindly happen for:

- User request shape errors.
- Missing model errors, unless another key may still support it.
- No enabled key available.

Every failure writes a request log.
Client errors should still be OpenAI-compatible and readable.

## Security Boundaries

Hard rules:

- Bind only to `127.0.0.1`.
- Do not bind to `0.0.0.0`.
- Do not expose admin APIs externally.
- Do not record prompt or response bodies.
- Do not record full keys.
- Do not log cookie / session / token values.
- Do not leak upstream secrets in error text.
- LAN access is a later explicit opt-in, not P5.

## Verification Plan

Manual acceptance:

1. Set the proxy port in Settings.
2. Start the proxy.
3. Run `curl http://127.0.0.1:<port>/v1/models`.
4. Send a non-streaming `/v1/chat/completions` request.
5. Disable the first key and confirm the next key is used.
6. Make the first upstream fail and confirm fallback.
7. Confirm the request appears in logs.
8. Stop the proxy and confirm the port is closed.
9. Restart the app and confirm proxy state follows settings.
10. Confirm no enabled key returns a readable error.
11. Confirm `stream=true` has explicit behavior.

## P5 Completion Standard

P5 is complete when:

- The app can run a local OpenAI-compatible endpoint.
- `/v1/models` and non-streaming `/v1/chat/completions` work locally.
- Enabled keys are selected by priority.
- Fallback works across keys.
- Request logs show real proxy metadata.
- The UI shows whether the proxy is running.
- The implementation stays local-only and secret-safe.
