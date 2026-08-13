# Architecture Scale Upgrade Stage 5 Task 20.B Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Base revision before this shard: `0a95746 refactor: move newapi collector parsers to driver tree`
- Shard: Stage 5 / Task 20.B, NewAPI collector production cutover
- Governing documents:
  - `docs/archive/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/archive/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The zero-diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/audits/persistence-v2-boundary-manifest.json` remained empty.
- UI/runtime inspection: not performed. This shard used source, Rust tests, provider conformance, parser-backed gates, and command output only.

## Shard Decision

Task 20.B cuts over only the NewAPI collector capability. NewAPI remote-key and authorization remain unsupported in the static registry and provider matrix.

The production collector path now prepares NewAPI session material outside the driver, passes only non-secret auth metadata plus an opaque secret accessor into the driver, and performs NewAPI collector HTTP through `AsyncOutboundClient`. The driver owns NewAPI collector endpoint requests, typed failure mapping, bounded/redacted evidence, canonical balance/group/rate/model facts, and usage-stat enrichment for balance collection.

The old generic collector dispatcher no longer calls `adapters::newapi::collect`; it returns an `async_driver_required` manual output if reached. Existing NewAPI adapter remote-key/auth code remains for later Task 20.C/20.D and Task 21/22 shards.

## Requirements Evidence

| Task 20 requirement | Current evidence | Result |
|---|---|---|
| NewAPI collector capability is static-registry owned | `drivers/mod.rs` declares NewAPI collector supported tasks `Detect`, `Balance`, `Groups`, and `Models` with `NewApiCollectorDriver`; remote-key and authorization remain `None`. | Pass |
| Driver uses shared async outbound, not `ureq` fallback | `drivers/newapi/mod.rs` builds typed `OutboundRequest` values and calls `AsyncOutboundClient::execute`; no production collector fallback from driver to legacy `ureq` exists. | Pass |
| Auth/session resolution stays outside the driver | `adapters/newapi::prepare_collector_auth_context` resolves existing session/password-login material during preparation. The driver receives `ProviderAuthContext::NewApi { user_id, secret_purpose }` and resolves only the matching short-lived secret through `DriverSecretAccessor`. | Pass |
| Collector route cutover covers manual and scheduled paths | `StationCollectionCommandFacade` finishes `PreparedStationCollectionRoute::NewApi` through `finish_newapi_collection_v2`; `V2StationCollectorTaskAdapter` finishes `PreparedStationTaskRoute::NewApi` through `finish_newapi_task_v2`. | Pass |
| Same capability has no legacy production string branch | `dispatch_adapter_output` no longer calls `adapters::newapi::collect`; NewAPI collector work must be routed through the capability driver path. | Pass |
| Existing NewAPI balance semantics keep usage stats | The async driver ports dashboard/log window collection and merges usage stats before parsing `CollectedBalanceFact`. Existing `newapi_` tests for usage logs, dashboard total rejection, token-display behavior, and remote-key/auth surfaces still pass. | Pass |
| Provider conformance matrix is supported only after fixtures exist | `provider-capability-matrix.json` marks NewAPI collector supported and `src-tauri/tests/fixtures/providers/manifest.json` provides all 11 required scenarios: success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancel, budget exhaustion, stale endpoint revision, and redaction. | Pass |
| Do not touch Persistence V2 | `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json` printed no diff. | Pass |

## Verification Commands

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass after formatting |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass, with dead-code warnings from legacy NewAPI adapter collector helpers that are no longer production-routed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml services::collectors::drivers::newapi --lib -- --nocapture` | Pass, 11 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml newapi_ --lib -- --nocapture` | Pass, 19 passed / 1 ignored live test |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` | Pass, 26 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` | Pass, 4 passed |
| `pnpm generate:bindings --check` | Pass, 4 artifacts / two-run deterministic |
| `pnpm exec tsc --noEmit` | Pass |
| `node scripts\architecture\check-command-state-boundaries.mjs` | Pass, 104 migrated commands |
| `node scripts\architecture\check-typescript-boundaries.mjs` | Pass, 939 resolved edges |
| `node scripts\architecture\check-build-entries.mjs` | Pass, 422 production modules / 246 demo modules |
| `node scripts\architecture\check-command-registry.mjs` | Pass, 125 commands generated |
| `node scripts\architecture\check-tauri-security.mjs` | Pass, 2 capabilities |
| `node scripts\architecture\check-artifact-policy.mjs` | Pass, 6 registered legacy roots |
| `node scripts\architecture\check-dependency-lifecycle.mjs` | Pass, 18 entries |
| `node scripts\architecture\check-fixtures.mjs` | Pass |
| Audit/fixture JSON parse | Pass |
| Persistence V2 zero-diff check | Pass |

## Residual Work

- Task 20.C must migrate NewAPI remote-key capability without weakening idempotency/result-unknown/reconciliation semantics.
- Task 20.D must migrate NewAPI authorization validation without moving WebView capture, cookie extraction, or persistence ownership into the driver.
- Stage 5 Gate remains open until the remaining Task 20 shards, Task 21, and Task 22 complete and the full Stage 5 gate passes.
