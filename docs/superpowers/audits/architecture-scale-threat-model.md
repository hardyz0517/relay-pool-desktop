# Architecture Scale Threat Model

## Scope

This model covers the local Tauri desktop process, its main and capture WebViews, local SQLite/credentials, provider and updater network boundaries, the local OpenAI-compatible proxy, generated IPC contracts, and shutdown/update races. Account systems, cloud tenancy, payments, and team authorization are outside the product boundary.

## Assets and trust boundaries

| Asset | Trust boundary | Required property |
|---|---|---|
| Provider API keys, cookies, sessions | UI -> Tauri -> credential store -> provider | never enter logs, fixtures, bundle, URL, metric label, or long-lived DTO |
| Local database and routing state | command/application -> persistence ports | authenticated local caller, atomic mutation, deterministic recovery |
| Main WebView authority | main renderer -> Tauri ACL/commands | least privilege, non-null CSP, no remote document authority |
| Capture session authority | remote capture WebView -> narrow commands | label/session/revision/exact-origin bound on every invoke |
| Provider responses | Internet -> outbound/parser/application | bounded body/time, typed provenance/completeness, no implicit success |
| Update artifacts | updater endpoint -> signature/bundle/relaunch | signed, revision-linked, race-safe shutdown and rollback evidence |
| Proxy requests and logs | local clients -> proxy -> provider | bounded/redacted diagnostics, no credential disclosure |

## Threat ledger

| ID | Abuse case | Existing control | Target control | Owner / evidence |
|---|---|---|---|---|
| TM-01 | Malicious or compromised provider returns oversized, malformed, or misleading partial data | typed parsers exist unevenly; request timeouts vary | shared request budget/body cap, capability-specific parser, completeness/evidence and `Unsupported`/typed failure | Tasks 14, 19-22; outbound fixtures and provider conformance |
| TM-02 | Lookalike origin or another station reuses a capture window/session | capture window label and sanitized event command | bind window label, station id, session id, revision and normalized exact origin on every invoke; reject suffix/substring/userinfo tricks | Task 17; exact-origin bypass matrix, Task 26/28 security gate |
| TM-03 | Redirect copies Authorization/Cookie to a new origin or downgrades HTTPS | provider-specific clients have inconsistent redirect rules | central redirect policy strips credentials cross-origin and rejects HTTPS downgrade; refresh is single-flight | Tasks 14, 19-22; redirect fixtures |
| TM-04 | Compromised remote capture renderer invokes broad main commands | capture capability is remote-only and currently exposes one custom permission | separate capture capability, two-command maximum, application validator, no main capability inheritance | Tasks 17/28; compiled ACL and live negative tests |
| TM-05 | Compromised main renderer exploits permissive content loading | local main window, Tauri ACL | production non-null CSP, fixed local production entry, external navigation owner, no demo graph/fallback | Task 8; build graph and final bundle scan |
| TM-06 | Stale WebView assets speak an incompatible IPC contract | command-not-found/text fallback exists | build hash/version handshake before feature mount; fail-closed recovery screen | Tasks 3-4/8; mismatch matrix |
| TM-07 | Secret appears in logs, tracing, fixture, report, metric label, panic, or bundle | ad hoc masking | secret wrapper/zeroize boundary, one redaction contract, canary scans, low-cardinality closed labels | Tasks 4/14/18/28; canary and bundle scan |
| TM-08 | Update or tray/window exit races with proxy/database/background work | direct exit and late `RunEvent::Exit` drain | idempotent `ExitCoordinator`, `ExitRequested` prevent, bounded async drain, one final exit, structured report | Task 17; race/fault/soak matrix |
| TM-09 | Lost response causes duplicate remote key creation | provider-specific behavior | idempotency key when supported, commit barrier, `ResultUnknown`, reconciliation before retry | Tasks 15/20/21; conformance matrix |
| TM-10 | Forced termination leaves partial local state or unrecoverable work | SQLite transactions and existing recovery tests | atomic mutation boundary, crash/restart qualification, no false cancelled/succeeded terminal | Tasks 15/27/28; crash matrix |
| TM-11 | Browser preview/mock silently replaces a failed desktop backend | widespread invoke-unavailable fallbacks | physically separate demo entry and deterministic `DemoBackend`; production bootstrap has no fallback | Tasks 8-9; bundle reachability and Tauri smoke |
| TM-12 | Unbounded work, response, diagnostics, or labels exhaust memory/handles | mixed threads/spawns and local buffers | bounded queues/rings/bodies/observers, cancellation/join owner, TTL/GC, saturation outcome | Tasks 14-18/27; capacity and soak reports |
| TM-13 | Local proxy is exposed beyond intended interface or logs sensitive payloads | fixed local endpoint and masking tests | explicit bind policy, secret-safe request diagnostics, bounded bodies and shutdown drain | Tasks 14/17/18/27; local lifecycle tests |
| TM-14 | Build/CI artifact or stale generated binding is shipped from a different revision | lockfiles and release workflow | zero-diff generation, source/candidate hash, shared verify entrypoint, signed bundle provenance | Tasks 2-3/26-28; release qualification |

## Current accepted debt

Only the following time-bounded debts are accepted at Stage 0: production `csp: null` until Task 8, production/browser-preview graph mixing until Task 8/9, capture wildcard URL shell and missing application exact-origin validation until Task 17, and late/direct shutdown until Task 17. Their machine-readable owners and expiries live in `architecture-scale-tauri-security-manifest.json`; expiration is a hard failure, not a review reminder.

## Security invariants

1. A remote document never gains main-window command authority.
2. URL authorization uses parsed normalized exact origins, never substring/suffix matching.
3. Redirect, retry, debug, and tracing layers cannot duplicate credentials.
4. A failed desktop handshake cannot become demo or mock success.
5. Cancellation only reports cancellation after commit is impossible or reconciled.
6. Shutdown is idempotent, bounded by one global deadline, and produces a redacted report.
7. Release evidence and bundle are produced from the same staged revision.
