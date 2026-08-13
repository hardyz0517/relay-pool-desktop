# Architecture Scale Upgrade Stage 5 Task 19.B Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Shard: Stage 5 / Task 19.B, provider conformance harness
- Governing documents:
  - `docs/archive/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/archive/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The audit diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/audits/persistence-v2-boundary-manifest.json` was empty.
- UI/runtime inspection: not performed. This shard used source, fixture manifests, matrix JSON, Rust integration tests, parser-backed gates, and command output only.

## Shard Decision

Task 19.B is implemented. The provider conformance harness now exists and fails closed against the Stage 19.A static registry and the provider capability matrix.

Because Task 19.A intentionally registered no concrete capability drivers, all Sub2API, NewAPI, and OpenAI-compatible capability entries are currently `unsupported`. The harness still encodes the complete supported-capability fixture requirement so Task 19.C and Tasks 20-21 cannot mark a capability supported without success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancel, budget exhaustion, stale endpoint revision, and redaction fixtures.

Task 19.C may start next. Stage 5 Gate is not claimed by this shard.

## Requirements Evidence

| Task 19.B requirement | Current evidence | Result |
|---|---|---|
| Create a provider conformance integration harness | `src-tauri/tests/provider_conformance.rs` compiles the provider contract/registry source, reads the capability matrix and fixture manifest, and validates registry/matrix/fixture consistency. | Pass |
| Create provider fixtures under `src-tauri/tests/fixtures/providers/**` | `src-tauri/tests/fixtures/providers/manifest.json` records unsupported fixtures for each current provider/capability pair. | Pass |
| Create `provider-capability-matrix.json` | `docs/audits/provider-capability-matrix.json` declares Sub2API, NewAPI, and OpenAI-compatible descriptors and collector/remote-key/authorization capability status. | Pass |
| Registered supported capabilities must run all required conformance fixture classes | `provider_fixtures_are_complete_for_declared_matrix` requires success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancel, budget exhaustion, stale endpoint revision, and redaction fixtures for any matrix entry marked `supported`. | Pass |
| Fixture manifest records provider kind, capability, endpoint role, request/response schema, redaction status, source/provenance, and expected facts/failure | The harness rejects blank or missing fixture structure, invalid provider/capability/endpoint role, non-object request/response schemas, invalid redaction status, missing source provenance, and unsupported fixtures without typed failure evidence. | Pass |
| Matrix unsupported status matches registry descriptor and runtime typed Unsupported | `provider_capability_matrix_matches_static_registry` compares matrix descriptors to `stage19a_static_entries`, then calls `collector`, `remote_key`, and `authorization` lookups to prove absent capabilities return `DriverFailureKind::Unsupported`. | Pass |
| Missing fixture for declared capability fails closed | The harness requires every matrix entry to cite at least one existing fixture; supported entries additionally require the full conformance scenario set. | Pass |
| Production provider paths are not cut over | Matrix status is `contract-foundation-no-production-provider-cutover`; every capability has `declaredInRegistry: false`, and production adapter files were not modified. | Pass |

## Verification Commands

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` | Pass, 13 passed |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass, 697 passed / 1 ignored |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` | Pass, 4 passed |
| `pnpm generate:bindings --check` | Pass, 4 artifacts, two-run deterministic |
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

- Task 19.C must add the OpenAI-compatible reference driver and update the matrix/fixtures from unsupported to supported for its applicable capability only after the full scenario suite passes.
- Tasks 20-21 must migrate NewAPI and Sub2API capability shards and delete their old string/`ureq` paths per capability.
- Task 22 still owns remaining management/probe HTTP cleanup.
- Stage 5 Gate remains open until provider-specific code is localized, conformance is complete for every supported capability, production `ureq` use is removed, and provider addition only touches registry/driver/fixtures.
