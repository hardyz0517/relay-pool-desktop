# Sub2API Adaptive Collection Recovery Design

Date: 2026-07-11
Status: Reviewed and approved for implementation planning

## Context

Automatic Sub2API collection currently applies different recovery behavior to balance and group requests. Group collection can refresh a rejected login token and retries transient endpoint failures once. Balance collection does not refresh a rejected login token on its account-level fallback path, and it records a failed snapshot immediately after the first collection attempt is exhausted.

The observed failure on 2026-07-10 demonstrates the mismatch:

- Balance collection received `401` from `/api/v1/user/profile` and `/api/v1/auth/me` and finished as failed.
- A subsequent collection cycle reached the group task, refreshed the login session, and succeeded.
- The next scheduled balance collection reused the refreshed token and succeeded.

Other recorded failures include one-shot `502` responses and persistent network timeouts. These failure classes require different recovery actions. A fixed retry count applied to every error would retry permanent failures, amplify login traffic, and let a single unreachable station block the sequential collector runner.

## Goals

- Use error-specific recovery instead of unconditional retries.
- Give balance and group collection consistent login-token recovery behavior.
- Retry transient failures within strict attempt and time limits.
- Emit collection failure events only after meaningful recovery is exhausted.
- Preserve enough redacted attempt data to explain later failures and recoveries.
- Keep the change local to Sub2API collection and reusable by other adapters later.

## Non-Goals

- Changing the scheduler interval or due-station selection.
- Adding collector concurrency, circuit breakers, or cross-station rate limiting.
- Adding retry settings to the UI.
- Changing database tables.
- Migrating NewAPI or OpenAI-compatible adapters in the same change.
- Treating a single balance endpoint `401` as proof that a station key is globally invalid.

## Selected Approach

Add a small request-recovery module used by the Sub2API balance and group collectors. The module owns attempt limits, error classification, retry delay, authentication recovery, task budget consumption, and redacted attempt tracing. Collector functions continue to own endpoint order, payload parsing, fact creation, and final task status.

This is preferred over adding separate loops to each collector function because separate loops would drift in behavior. A scheduler-level state machine is deferred because the current evidence supports an adapter request-recovery problem, not a scheduler rewrite.

## Components

### RequestPolicy

`RequestPolicy` defines production limits:

- At most three attempts per endpoint for transient failures.
- At most one authentication recovery sequence per collection task.
- A 30-second budget shared by all endpoints in one balance or group task.
- Retry delays of approximately 300 ms and 1 second, with small jitter in production.
- One budget-aware authentication recovery sequence. The sequence may try the existing login path and username-field candidates, but it cannot restart from the beginning.

Three attempts are an upper bound, not a required count. Permanent failures, rejected refreshed credentials, missing credentials, and exhausted budgets stop earlier.

Tests use an injected policy with zero delays and a controllable clock so the suite remains deterministic and fast.

### CollectionAttemptBudget

One budget instance is created at the start of each balance or group task and passed through every endpoint execution. It tracks the deadline and prevents a sequence of individually bounded requests from creating an unbounded task.

Before an attempt, the executor derives its request timeout from the lesser of the existing request timeout and the remaining task budget. When no useful budget remains, it returns `task_budget_exhausted` without sending another request.

The budget also applies to login transport. The existing login transport can probe up to three paths with three username-field variants. Those probes are candidate compatibility checks inside the single recovery sequence, not separate authentication recovery sequences. Candidate requests use the remaining task budget. A network failure or `5xx` may retry the same candidate once and then ends recovery if that candidate still fails transiently. A login `429` follows `Retry-After` only when the delay fits the remaining budget. Permanent candidate-level responses may continue to the next compatible path or field while budget remains.

For stations with multiple routeable keys, budget allocation must preserve first-attempt fairness. The collector sends one `/v1/usage` attempt for every key that can be reached within the budget before starting transient retry rounds. Transiently failed keys are queued for at most two later retry rounds. A failing early key therefore cannot consume all three attempts before a later healthy key receives its first attempt.

### RequestExecution

The shared executor accepts:

- A request operation that uses the current credential.
- An optional authentication recovery operation.
- The shared task budget.
- The request policy.

It returns:

- The final redacted endpoint result.
- Attempt count and attempt records.
- The final failure kind, when any.
- The ordered recovery actions that produced success, when any.
- The latest credential after authentication recovery.

The executor never logs or returns a password, token, Cookie, Authorization header, or unredacted response body.

### Shared Login State

Account-level balance endpoints and group endpoints use a mutable task-local login state. If one endpoint refreshes the access token, all later endpoints in that task use the refreshed token. The refreshed token is also stored through the existing credential persistence boundary.

The task may start login recovery only once. The recovery sequence can accept and persist at most one replacement token. A refreshed token that is rejected again ends as `auth_rejected`; it cannot trigger another recovery sequence.

## Error Classification

| Condition | Classification | Recovery |
|---|---|---|
| `401` or `403` using login token | `auth_rejected` | Refresh login once, then retry original endpoint once |
| `401` or `403` using API key | `auth_rejected` | Do not repeat the same key request; continue to account-balance fallback |
| Network error or timeout | `network_timeout` | Retry within attempt and task budgets |
| HTTP `408` | `network_timeout` | Retry within budgets |
| HTTP `429` | `rate_limited` | Respect `Retry-After` only when it fits the remaining task budget |
| HTTP `500..=599` | `upstream_5xx` | Retry within budgets |
| HTTP success with malformed JSON | `invalid_json` | Retry once within the task budget |
| Valid JSON without required business fields | `unsupported_payload` | Do not repeat the same endpoint; continue an available business fallback |
| HTTP `400`, `404`, or `422` | Permanent HTTP failure | Do not retry |
| Missing or undecryptable credentials | `credential_unavailable` | Return `manual_required` when user action can resolve it |
| No remaining task budget | `task_budget_exhausted` | Stop without another request |

Other `4xx` responses default to non-retryable unless a future adapter-specific rule explicitly classifies them.

## Balance Collection Flow

For each routeable station key, `/v1/usage` executes with transient recovery but without login recovery:

1. The first pass gives every reachable key one attempt before any key receives a transient retry.
2. A successful response with a usable balance creates a key balance fact.
3. Transiently failed keys enter at most two later retry rounds within the shared balance-task budget.
4. A key `401/403` is not retried with the same key.
5. Permanent key endpoint failures proceed to the existing account-level fallback when no key balance facts were collected.

The account-level fallback uses a shared login state:

1. Request `/api/v1/user/profile`.
2. On login-token `401/403`, perform one login recovery and retry `/user/profile` with the refreshed token.
3. On a successful payload without a recognized balance field, try `/api/v1/auth/me` using the latest token.
4. Apply the same transient classification and shared task budget to both endpoints.
5. Finish as failed only when no usable balance fact remains after all meaningful recovery.

## Group Collection Flow

`/api/v1/groups/available` and `/api/v1/groups/rates` use the same task-local login state and task budget:

1. Either endpoint may trigger the single allowed login refresh.
2. A token refreshed by the first endpoint is used by the second endpoint.
3. Each endpoint may retry transient failures up to the policy limit while budget remains.
4. Existing partial-result behavior is preserved: if valid group facts exist but one rate endpoint remains unavailable, the task may finish as `partial` rather than discarding useful facts.

## Final Status and Change Events

Each collection task creates one final snapshot. Intermediate attempts are embedded in that snapshot and do not create snapshots or change events of their own.

- `success`: the task produced its required usable facts.
- `partial`: the task produced usable partial facts and the adapter contract permits partial output.
- `manual_required`: user-resolvable credentials or station-side action are required.
- `failed`: meaningful recovery was exhausted without usable facts.

When no usable facts exist, final error precedence is explicit:

1. `task_budget_exhausted` wins when the task stops because no budget remains.
2. A replacement token that is also rejected finishes as `auth_rejected`.
3. A login transport failure finishes as `auth_refresh_failed` and retains a redacted cause such as `network_timeout`, `rate_limited`, or `upstream_5xx` in diagnostics.
4. Missing or undecryptable saved credentials produce `manual_required/credential_unavailable` only when authentication recovery is the remaining user-resolvable path.
5. Exhausted transient endpoint failures keep their endpoint classification.

For multi-key balance collection, any usable balance fact preserves the existing overall `success` behavior. Failed sibling keys remain visible in endpoint diagnostics; this design does not redefine the task as partial.

A retry or authentication refresh that succeeds within the same task finishes as `success` or `partial` and does not create a `collector_failed` or `collector_recovered` event. The snapshot records the internal recovery.

Only a final `failed` snapshot creates `collector_failed`. Existing deduplication remains responsible for merging repeated failures. When the previous finished run for the same station and task was failed and the next run finishes as success or partial, the existing `collector_recovered` behavior remains unchanged.

## Snapshot Diagnostics

Endpoint diagnostics remain in the existing JSON fields; no schema migration is required. Each endpoint result adds fields equivalent to:

```json
{
  "url": "https://redacted.example/api/v1/user/profile",
  "path": "/api/v1/user/profile",
  "attemptCount": 2,
  "status": 200,
  "ok": true,
  "durationMs": 1428,
  "failureKind": null,
  "recoveryActions": ["auth_refresh"],
  "attempts": [
    {
      "attempt": 1,
      "status": 401,
      "durationMs": 708,
      "action": "refresh_auth"
    },
    {
      "attempt": 2,
      "status": 200,
      "durationMs": 720,
      "action": "complete"
    }
  ]
}
```

The top-level endpoint result remains one record per logical endpoint, not one record per attempt. It must retain the existing `status`, `ok`, and `durationMs` fields, plus the existing `url` or `endpoint` field used by that collector path. `durationMs` is total wall-clock time for the logical endpoint, including retry delays and authentication recovery; each nested attempt keeps its own request duration. `endpoint_counts_from_summary` depends on top-level `ok`; nested attempts must not inflate endpoint, success, or failure counts. New `path`, `attemptCount`, `failureKind`, `recoveryActions`, and `attempts` fields are additive.

Allowed `recoveryActions` entries are `auth_refresh` and `transient_retry`, in execution order. The array is absent when no recovery action ran and can contain both values when authentication recovery is followed by a transient retry. Stable final error codes include:

- `auth_rejected`
- `auth_refresh_failed`
- `network_timeout`
- `rate_limited`
- `upstream_5xx`
- `invalid_json`
- `unsupported_payload`
- `credential_unavailable`
- `task_budget_exhausted`

Attempt records contain only endpoint paths, attempt numbers, status codes, durations, retry delays, classifications, and recovery actions. Existing redaction remains a defense-in-depth boundary before snapshot persistence.

## Test Design

Focused Rust tests use the existing local flaky HTTP fixture and synthetic credentials. They must cover:

1. Account balance receives `401`, login succeeds, the retried endpoint returns `200`, the new token is persisted, and the final result is success without a failure event.
2. A refreshed token is also rejected, the authentication recovery sequence starts exactly once, and the final result is `auth_rejected`.
3. Missing saved credentials produce `manual_required` without repeated endpoint requests.
4. `502` followed by `200` succeeds through `transient_retry`.
5. A network error followed by success is retried within budget.
6. Repeated `5xx` responses stop after three endpoint attempts and create one final failed snapshot and event.
7. `429` respects a usable `Retry-After` value and stops when the delay would exceed the remaining budget.
8. `400`, `404`, and `422` are attempted once.
9. Malformed JSON retries once; a valid payload without a required field does not repeat the same endpoint.
10. A token refreshed during `/user/profile` is reused by `/auth/me` and later group requests.
11. An endpoint that refreshes authentication, then receives `502`, then succeeds records both recovery actions in order.
12. With multiple station keys, every reachable key receives one initial attempt before an earlier transient failure receives its second attempt.
13. Login candidate probing and any candidate retry consume the same collection-task budget and cannot restart the recovery sequence.
14. Budget exhaustion prevents further endpoint requests and produces `task_budget_exhausted`.
15. Top-level endpoint `ok` remains accurate, nested attempts do not inflate run counts, and a recovered endpoint contributes one successful endpoint.
16. Snapshot and error serialization contain no supplied token, password, Cookie, or Authorization value.
17. A successful internal retry records attempt diagnostics but emits no failure or recovery change event.
18. A final success after a previous failed run retains the existing task-specific recovery event behavior.

Verification commands:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
node scripts\station-auto-collector.test.mjs
```

## Implementation Boundaries

The implementation plan should keep the production change narrowly scoped to:

- A small request-recovery module under the collector adapter boundary.
- Sub2API balance and group integration.
- A budget-aware extension to the existing Sub2API login transport in `src-tauri/src/services/collectors/sub2api.rs`.
- Focused Rust regression tests and any required update to the existing station auto-collector script.

It must not introduce UI controls, database migrations, scheduler concurrency, circuit breakers, unrelated adapter migrations, or unrelated collector refactors.

## Success Criteria

- The observed balance `401` followed by group login recovery is resolved within the balance task itself.
- One-shot transient failures can recover without creating false failure and recovery events.
- Permanent failures are not retried.
- Persistent network failures cannot exceed the task-level time budget.
- Refreshed credentials are shared within a task and never refreshed in a loop.
- Multiple routeable keys receive a fair initial attempt before transient retries.
- Existing endpoint success and failure counts remain compatible.
- Attempt diagnostics explain the final decision without exposing secrets.
- Existing group partial semantics and collector recovery events remain compatible.
