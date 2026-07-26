# Architecture Scale Upgrade Stage 4 Gate Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Audited revision: `0ffdddb1621f8186cbb3fb52e4bbe6c805f1c69d`
- Audit time: 2026-07-27 01:24:32 +08:00
- Governing documents:
  - `docs/superpowers/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/superpowers/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The audit diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/superpowers/audits/persistence-v2-boundary-manifest.json` was empty.
- UI/runtime inspection: not performed. This gate used source, compiled contracts, parser-backed gates, and command/test output only.

## Gate Decision

Stage 4 Gate is passed for the audited revision.

Stage 5 may start from `0ffdddb1621f8186cbb3fb52e4bbe6c805f1c69d`, subject to the Stage 5 shard rules in the master plan. Stage 5 must still remove or replace the explicitly owned legacy provider/probe/updater HTTP and blocking allowlist entries assigned to Tasks 20-22; those entries are not treated as Stage 4 failures because the parser-backed manifest gives each one an exact owner, reason, delete shard, and expiry stage.

## Requirements Evidence

| Stage 4 Gate requirement | Current evidence | Result |
|---|---|---|
| Daemon, operation, and blocking owners are separated | `TaskSupervisor`, `OperationRegistry`, and `BlockingExecutor` have separate focused integration tests and separate production owners in `background_tasks`; `architecture_scale_boundaries` passed against the manifest. | Pass |
| Governed work is bounded, cancellable, waitable, and diagnosable | `task_supervisor`, `blocking_executor`, `operation_registry`, `observability_contract`, and `observability --lib` passed. Tests cover capacity, queue timeout, cancel, terminal states, orphan diagnostics, and visible low-cardinality diagnostics. | Pass |
| Shared async outbound is available for Stage 5 | `async_outbound` passed with client pooling, proxy policy, redirect/header stripping, typed timeout/failure, body limit, streaming chunk delivery, cancellation, and redacted evidence. | Pass |
| Supervisor uses Tokio primitives, not a custom runtime | `task_supervisor` and `architecture_scale_boundaries` passed; parser gate permits owned runtime spawns and rejects stale/expired allowlist entries. No custom executor/workflow gate failure remains. | Pass |
| Real exit entries unify through `ExitCoordinator`; `RunEvent::Exit + block_on` is not the primary shutdown path | Source audit found tray quit, true-close, and `RunEvent::ExitRequested` routed through `ExitCoordinator::request_exit`; `RunEvent::Exit => {}`. Setup-time `block_on` calls remain but are not the primary exit drain. | Pass |
| Capture capability is separated from main and guarded by application checks | `src-tauri/capabilities/capture.json` is scoped to `capture-*` with `record-capture-event`; `src-tauri/capabilities/default.json` is scoped to `main` with `main-window`. `check-tauri-security.mjs` passed. | Pass |
| Task 18 observability, redaction, runtime status, diagnostics exposure, and operation event envelope are frozen | `observability_contract`, `observability --lib`, `operations --lib`, and `ipc::registry --lib` passed. Registry test proves `get_runtime_status` is the only public runtime diagnostics surface; operation DTO tests prove explicit event id/version/sequence/terminal. | Pass |
| Demo build graph cannot reach desktop bridge/Tauri assets | An earlier final gate run failed here. Commit `24008d6` isolated frontend `RuntimeStatus` shared types from generated IPC DTO imports; commit `0ffdddb` moved shared updater/change helpers out of feature-private paths. Final `check-build-entries.mjs` passed with `422 production modules, 246 demo modules`. | Pass |
| Boundary manifest stage field is current | `architecture-scale-boundary-manifest.json` now has `current_stage: 4`. The expired Stage 3 temporary TypeScript edge group owned by Tasks 12-13 was removed, shared updater/change helpers were moved out of feature-private paths, and parser/TypeScript gates were rerun so Stage 4 is not relying on an expired allowlist. Later-deletion Stage 5/6/8 entries remain owned and unexpired. | Pass |
| Persistence V2 not modified | `git diff -- src-tauri/src/persistence src-tauri/migrations docs/superpowers/audits/persistence-v2-boundary-manifest.json` produced no output before and after final verification. | Pass |

## Final Verification Commands

All commands below were run after commit `0ffdddb` on the audited revision.

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test task_supervisor -- --nocapture` | Pass, 8 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test blocking_executor -- --nocapture` | Pass, 6 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test operation_registry -- --nocapture` | Pass, 9 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test async_outbound -- --nocapture` | Pass, 10 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test observability_contract -- --nocapture` | Pass, 16 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` | Pass, 4 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml observability --lib -- --nocapture` | Pass, 25 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml operations --lib -- --nocapture` | Pass, 12 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml ipc::registry --lib -- --nocapture` | Pass, 26 passed |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass |
| `pnpm generate:bindings --check` | Pass, 4 artifacts, two-run deterministic |
| `pnpm exec tsc --noEmit` | Pass |
| `node scripts\architecture\check-command-registry.mjs` | Pass, 125 commands |
| `node scripts\architecture\check-command-state-boundaries.mjs` | Pass, 104 migrated commands |
| `node scripts\architecture\check-typescript-boundaries.mjs` | Pass, 939 resolved edges |
| `node scripts\architecture\check-tauri-security.mjs` | Pass, 2 capabilities |
| `node scripts\architecture\check-build-entries.mjs` | Pass, 422 production modules and 246 demo modules |
| `node scripts\architecture\check-artifact-policy.mjs` | Pass, 6 registered legacy roots |
| `node scripts\architecture\check-dependency-lifecycle.mjs` | Pass, 18 entries |
| `node scripts\architecture\check-fixtures.mjs` | Pass |
| `pnpm exec vitest run src/features/data-recovery/DataStoreBootstrap.test.tsx src/app/bootstrap/BackendBootstrap.test.tsx src/lib/bridge/DemoBackend.test.ts src/lib/api/runtimeStatus.test.ts` | Pass, 4 files / 12 tests |
| JSON parse check for architecture/persistence audit manifests | Pass |
| Persistence V2 zero-diff check | Pass |

Expected panic text was printed by `should_panic` and panic-classification tests in `task_supervisor`, `blocking_executor`, and observability correlation tests; those test binaries still exited successfully.

## Residual Work Not Claimed By This Gate

- Stage 5 has not started in this audit commit. Provider capability contracts, conformance harnesses, NewAPI/Sub2API driver cutovers, and production `ureq` removal remain Tasks 19-22.
- Stage 6 physical decomposition and deletion remain Tasks 23-25.
- Stage 7 qualification remains Tasks 26-28, including native/runtime performance and release evidence that the Stage 3 baseline explicitly deferred.
