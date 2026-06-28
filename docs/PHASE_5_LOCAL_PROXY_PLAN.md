# Phase 5 Local Proxy MVP

**Status:** P5.0-P5.4 core work has landed as the local gateway line. The app now exposes a localhost OpenAI-compatible proxy, routes through the enabled Key Pool, writes request logs, surfaces proxy status in the UI, supports `/v1/responses`, passes through SSE streams for chat completions and responses, handles CORS preflight, aggregates `/v1/models`, and shows request-log-derived Key / Channel status. Remaining work is deeper policy, model mapping, timeout tuning, and long-term health scoring.

**Goal:** Keep Relay Pool Desktop usable as a local OpenAI-compatible gateway that external tools can point at once, while the app chooses enabled Station Keys, forwards, falls back, and records what happened.

**Architecture:** Keep the existing Station / Station Key / Key Pool / Collector split. The proxy is a Rust HTTP server owned by Tauri state. Routing is key-first, station-second, local-only, and fallback-aware. The shipped P5 line covers request forwarding, Responses compatibility, SSE passthrough, OpenAI-style errors, model-list aggregation, CORS, and real request logging. The deeper policy engine stays out of P5.

**Tech Stack:** Tauri 2, Rust, SQLite, existing station/key/collector models, request log storage, existing React/TypeScript front end, local HTTP server in Rust.

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

### P5.0 Results

P5 should produce:

- Local server lifecycle control.
- `GET /v1/models`.
- `POST /v1/chat/completions` without streaming first.
- Enabled-key selection by priority.
- Retryable fallback across keys.
- Request logs with real proxy metadata.
- UI indicators for proxy running / stopped / error states.

### Current Implementation Notes

- `GET /v1/models` aggregates enabled upstream model lists and deduplicates by model id in Key Pool priority order.
- `POST /v1/chat/completions` is wired for non-streaming requests and SSE stream passthrough.
- `POST /v1/responses` is a first-class route for direct upstream forwarding, with non-streaming fallback/wrapping for chat-only upstreams.
- `POST /v1/responses` with `stream: true` is passed through directly when the selected upstream supports it.
- Station Key priority is used as the base order.
- Requests fall back on retryable upstream failures.
- Request logs store only metadata and redacted errors.
- Settings, dashboard, request logs, Key Pool, and Channel Status now read runtime/log-derived state.

## Non-Goals

P5 does not implement:

- Complex price-optimal routing.
- Full strategy groups.
- Allowlist / blocklist model policy.
- SSE event rewriting or mid-stream fallback after bytes have been sent.
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

Supported in the current P5 line:

- `POST /v1/responses`
- SSE passthrough for `stream: true` on `/v1/chat/completions`
- SSE passthrough for `stream: true` on `/v1/responses`
- CORS / `OPTIONS` preflight for browser-style clients

Optional later:

- `POST /v1/embeddings`

Streaming is passthrough, not event rewriting. Fallback can happen before a successful stream response is selected; once bytes are flowing, the proxy does not silently switch keys mid-stream.

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

Current P5 adds small status fields:

- Last used time.
- Last checked time.
- Basic health state from key status.

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

### Channel Status

P5 uses Key Pool rows plus real request logs to show:

- Station Key / Channel name.
- Owning station.
- Recent request outcomes.
- Average duration from request logs.
- Last used / last checked metadata.
- Latest redacted error summary.

This is not yet a full health engine or circuit breaker.

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
4. Confirm `/v1/models` returns a deduplicated OpenAI-style model list.
5. Send a non-streaming `/v1/chat/completions` request.
6. Disable the first key and confirm the next key is used.
7. Make the first upstream fail and confirm fallback.
8. Confirm the request appears in logs.
9. Confirm Channel Status reflects request-log-derived outcomes.
10. Stop the proxy and confirm the port is closed.
11. Restart the app and confirm proxy state follows settings.
12. Confirm no enabled key returns a readable error.
13. Confirm `stream=true` returns SSE for chat completions and responses when the upstream supports it.
14. Confirm browser-style `OPTIONS` preflight receives a local CORS response.

## P5 Completion Standard

P5 is complete when:

- The app can run a local OpenAI-compatible endpoint.
- `/v1/models`, `/v1/chat/completions`, and `/v1/responses` work locally for non-streaming requests.
- `/v1/models` aggregates and deduplicates enabled upstream model lists.
- `/v1/chat/completions` and `/v1/responses` can pass through upstream SSE streams.
- Enabled keys are selected by priority.
- Fallback works across keys.
- Request logs show real proxy metadata.
- The UI shows whether the proxy is running.
- Request logs, Key Pool, and Channel Status reflect real proxy traffic metadata.
- The implementation stays local-only and secret-safe.

## Next P5 Steps

- P5.5/P6: add deeper model mapping, timeout tuning, long-term health scoring, circuit breaker behavior, and policy editing.
