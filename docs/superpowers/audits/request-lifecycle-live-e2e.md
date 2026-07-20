# Request Lifecycle Live E2E Audit

状态：pending real authenticated run

日期：2026-07-20

## Scope

This audit is the Task 16 gate for the request lifecycle architecture upgrade. It must use the real Tauri application, the actual local routing port opened by that app, a user-approved configured station/key, and the local bearer provided through the current process environment.

Unit tests, loopback executor tests, fake upstream-only fixtures, or smoke-only soak output do not satisfy this gate.

## Verification scripts

```powershell
$env:RELAY_POOL_LOCAL_BEARER = "<redacted local bearer>"
$env:RELAY_POOL_E2E_MODEL = "<model served by the configured station>"
powershell -ExecutionPolicy Bypass -File scripts/verify-local-routing-lifecycle.ps1
```

For focused SQLite verification of a captured request id:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/verify-request-lifecycle-db.ps1 `
  -DatabasePath "$env:APPDATA\dev.relaypool.desktop\relay-pool-desktop.sqlite3" `
  -RequestId "<x-relay-request-id>"
```

Both scripts redact bearer/API-key/cookie-shaped values from output. The local bearer is never read from SQLite by the HTTP script and is never printed.

## Required real request matrix

| Case | Endpoint | Expected evidence |
|---|---|---|
| models | `GET /v1/models` | HTTP terminal plus one request row; partial/all upstream success reflected by attempt rows. |
| chat-non-stream | `POST /v1/chat/completions` | Buffered completion; request terminal and normalized attempt terminal agree. |
| chat-stream | `POST /v1/chat/completions` with `stream=true` | SSE terminal (`[DONE]` or protocol terminal) and body lease release after EOF. |
| responses-non-stream | `POST /v1/responses` | Native Responses buffered terminal is not inferred from HTTP 2xx alone. |
| responses-stream | `POST /v1/responses` with `stream=true` | `response.completed`/`failed`/`incomplete` controls protocol terminal; EOF alone must not become success. |
| embeddings | `POST /v1/embeddings` | Buffered envelope finalizes request and selected attempt exactly once. |
| stream cancel | streaming request cancelled by client | Delivery terminal records downstream interruption without corrupting upstream health. |
| fallback success | controlled A failure then B success | A and B each have attempt rows; health effects differ according to typed classification. |

The current script covers the first seven cases. The controlled fallback-success case still requires a real configured two-candidate route where the first candidate can be safely failed without leaking secrets or changing unrelated user data.

## SQLite assertions

`scripts/verify-request-lifecycle-db.ps1` enforces:

- exactly one `request_logs` row for the `x-relay-request-id`;
- non-empty request terminal fields;
- `request_attempts` count equals `request_logs.attempt_count`;
- attempt ordinals are contiguous from `0`;
- no duplicate `(request_id, ordinal)` attempt rows;
- `attempts_json` is not written unless an explicit compatibility flag is passed.

## Evidence log

No real authenticated run has been recorded in this audit file yet. The gate remains open until a redacted run captures request ids and matching SQLite summaries from the actual desktop app.

### 2026-07-20 local preflight

- `RELAY_POOL_LOCAL_BEARER` was not present in the current process environment.
- `RELAY_POOL_E2E_MODEL` was not present in the current process environment.
- `scripts/verify-local-routing-lifecycle.ps1 -Smoke -SkipDbVerify` correctly failed before sending HTTP because the bearer was missing.
- `scripts/verify-request-lifecycle-db.ps1` connected to the real app SQLite path but failed closed because the currently initialized live database did not yet expose the new terminal columns (`terminal_kind`, `terminal_code`, `terminal_detail`, `protocol_completed`, `delivery_terminal`, `selected_attempt_ordinal`). This indicates the live app/schema must be started/migrated from the upgraded build before Task 16 can pass.
