# Architecture Scale Upgrade Stage 5 Task 19.C Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Base revision before this shard: `1d934212fdd78054bef76687456821b89cfc1753`
- Shard: Stage 5 / Task 19.C, OpenAI-compatible reference collector driver cutover
- Governing documents:
  - `docs/archive/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/archive/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The audit diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/audits/persistence-v2-boundary-manifest.json` was empty.
- UI/runtime inspection: not performed. This shard used source, typed contracts, fixture manifests, Rust tests, parser-backed gates, and command output only.

## Shard Decision

Task 19.C is implemented. OpenAI-compatible collector capability is now the Stage 5 reference driver, registered through the static ProviderRegistry and executed through the Stage 4 `AsyncOutboundClient` for `/v1/models`. The old synchronous OpenAI-compatible collector adapter was deleted, and production collection paths now use provider-aware routing so supported OpenAI-compatible collector tasks go through the capability driver and unsupported tasks fail without falling back to provider-specific adapter code.

Only the OpenAI-compatible collector capability is marked supported. OpenAI-compatible remote-key and authorization capabilities remain explicitly unsupported. NewAPI, Sub2API, endpoint ping, web authorization, updater, and remaining production `ureq` cleanup are still owned by Tasks 20-22.

Stage 5 Gate is not claimed by this shard.

## Requirements Evidence

| Task 19.C requirement | Current evidence | Result |
|---|---|---|
| OpenAI-compatible has a concrete collector driver | `src-tauri/src/services/collectors/drivers/openai_compatible/mod.rs` implements `CollectorDriver` for `Detect` and `Models`, builds a typed GET `/v1/models` outbound request, parses `data[].id`, and maps HTTP/outbound failures to `DriverFailureKind`. | Pass |
| Driver uses shared async outbound, not synchronous provider HTTP | The driver accepts `AsyncOutboundClient` through `CollectorContext` and builds `OutboundRequest` with `RequestBudget`, `ProxyPolicy`, `CancellationToken`, sensitive Authorization header policy, and correlation id. No `ureq` is used in the OpenAI-compatible driver. | Pass |
| Old OpenAI-compatible collector adapter path is deleted | `src-tauri/src/services/collectors/adapters/openai_compatible.rs` was removed and `adapters/mod.rs` no longer exports it. The legacy dispatcher branch now only returns a defensive `async_driver_required` manual output if incorrectly reached. | Pass |
| Static registry declares only supported OpenAI-compatible collector capability | `drivers/mod.rs` registers `OpenAiCompatibleCollectorDriver` and declares supported tasks `Detect` and `Models`; remote-key and authorization remain absent and return typed Unsupported. Composition tests cover the reference collector-only registration. | Pass |
| Manual station collection route cuts over to the driver | `StationCollectionCommandFacade` now prepares a route and finishes OpenAI-compatible collections through `finish_openai_compatible_collection_v2` with the composed ProviderRegistry and outbound client before applying results. | Pass |
| Scheduled station collector route no longer falls back to the old adapter | `V2StationCollectorTaskAdapter` now prepares provider-aware routes. Supported OpenAI-compatible prepared tasks are finished through `finish_openai_compatible_task_v2` using the task cancellation token and correlation id; unsupported scheduled Balance/Groups tasks produce typed manual-required outputs instead of using the deleted legacy adapter. | Pass |
| Full collection keeps parent/child semantics | OpenAI-compatible `Full` prepares a Models child task. Regression `openai_driver_output_keeps_child_task_for_full_parent` proves the child output remains `Models` while the aggregate parent output remains `Full`. | Pass |
| Raw credentials remain behind a short-lived accessor | Preparation resolves the selected enabled station key into a non-Debug `StaticSecretAccessor` scoped to one prepared driver collection; the driver receives only `OpaqueCredentialHandle` and calls `DriverSecretAccessor` at request time. `CredentialSecret` remains non-Clone/non-Debug and zeroized. | Pass |
| Driver failures and evidence are typed and redacted | Driver tests cover auth, rate-limit, and server failure mapping. Production `redact_value` now redacts embedded `sk-...` material inside JSON error strings before sanitized detail is persisted or returned. | Pass |
| Provider capability conformance is updated | `provider-capability-matrix.json` marks `openai-compatible.collector` supported and cites the required success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancel, budget exhaustion, stale endpoint revision, and redaction fixtures in `src-tauri/tests/fixtures/providers/manifest.json`. | Pass |
| No Persistence V2 files changed | `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json` printed no diff. | Pass |

## Verification Commands

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` | Pass, 15 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml openai_compatible --lib -- --nocapture` | Pass, 2 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml openai_driver_output --lib -- --nocapture` | Pass, 1 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass, 701 passed / 1 ignored |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` | Pass, 4 passed |
| `pnpm generate:bindings --check` | Pass, 4 artifacts, two-run deterministic. First attempt timed out at 240s with no residual pnpm/cargo/rustc process; the successful rerun used a longer timeout. |
| `pnpm exec tsc --noEmit` | Pass |
| `node scripts\architecture\check-command-state-boundaries.mjs` | Pass, 104 migrated commands |
| `node scripts\architecture\check-typescript-boundaries.mjs` | Pass, 939 resolved edges |
| `node scripts\architecture\check-build-entries.mjs` | Pass, 422 production modules and 246 demo modules |
| `node scripts\architecture\check-command-registry.mjs` | Pass, 125 commands |
| `node scripts\architecture\check-tauri-security.mjs` | Pass, 2 capabilities |
| `node scripts\architecture\check-artifact-policy.mjs` | Pass, 6 registered legacy roots |
| `node scripts\architecture\check-dependency-lifecycle.mjs` | Pass, 18 entries |
| `node scripts\architecture\check-fixtures.mjs` | Pass |
| JSON parse check for audit manifests and provider fixture manifest | Pass |
| Persistence V2 zero-diff check | Pass |

## Residual Work

- Task 20 must migrate NewAPI collector, remote-key, and authorization capabilities before any NewAPI legacy adapter deletion.
- Task 21 must migrate Sub2API collector and remote-key capabilities before Sub2API legacy path deletion.
- Task 22 still owns endpoint ping, channel probe, web authorization HTTP validation, updater direct HTTP, remaining provider/probe `ureq` removal, and final deletion of provider string dispatchers.
- Stage 5 Gate remains open until Tasks 20-22 pass their own conformance, behavior, architecture, redaction, and Persistence V2 boundary gates.
