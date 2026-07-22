# Persistence Architecture V2 Qualification

Date: 2026-07-22
Branch: `codex/persistence-v2-upgrade`
Specification: `docs/superpowers/specs/2026-07-18-persistence-architecture-v2-upgrade-design.md`
Plan tasks: Task 15, Task 16

## Qualification status

**Status: source-qualified for Task 15; not release-qualified until Task 17.**

The V2 source architecture is substantially qualified in this worktree. Production recovery uses the same `RecoveryPlanner` and `RecoveryExecutor` boundary exercised by tests; config, journal, activation, tombstone, cleanup, import, lease release, runtime drain, runtime close, and bounded write-queue behavior have deterministic coverage. Proxy composition is now explicit: startup converts `services.request_finalization` to `Arc<dyn RequestLifecycleStore>` and supplies it to `ProxyStartConfig`; `services/proxy/runtime.rs` does not construct or import `RequestFinalizationService`.

Task 15's formal paired performance gate is now complete. The retained evidence is `D:\Dev\Build\relay-pool-persistence-v2-qualification\paired-v0.3.1-v2.json` with `qualificationStatus = qualified-paired-run`, release/locked execution, frozen V1 provenance, raw samples, and same-machine environment metadata. `hotRequestLogs` V2 p95 is 1,150,000 ns versus reconstructed V1 2,634,500 ns (10% maximum 2,897,950 ns); `hotChangeEventsFirstPage` and `startupWithoutMigration` also pass their relative gates. Final source contracts, frontend tests/build, and tracked artifact checks are green; a final candidate bundle scan and Task 17 remain separate release gates. Task 17 is externally unexecuted: a signed V2 Windows candidate, signed `v0.3.1` updater, isolated profiles/VM, fresh install, upgrade/downgrade, kill/disk-full/recovery UI, and live Proxy/Collector/Monitor verification are required.

## Current verified evidence

| Gate | Result | Scope / limitation |
| --- | ---: | --- |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | passed | Final Rust formatting check in this lane. |
| Strict Clippy (`cargo clippy --all-targets -- -D warnings`) | passed | No global warning allow was introduced. |
| `cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --nocapture` | passed | Final Rust all-targets run completed successfully in this lane. |
| `persistence_architecture` | 35 / 35 passed | Boundary manifest, V2 dependency edges, legacy absence, and visibility constraints. |
| `scripts/prepare-sqlx.ps1 -Check` | passed | Fresh SQLx offline metadata check. |
| `pnpm verify:persistence-artifacts` | passed | Tracked-worktree policy scan; this is not a generated release-bundle scan. |
| `persistence::differential_tests` | 11 / 11 passed | Real crate-internal V2 differential contract, not an integration-crate include facade. |
| Recovery, upgrade, lifecycle, queue, and fault focused suites | passed in prior/current lane evidence | Preserved regression evidence; full Rust all-targets includes their final source snapshot. |

The local-proxy contract now asserts the accepted boundary: startup injects `Arc<dyn RequestLifecycleStore>` and proxy runtime does not depend on the concrete finalization service. The corrected contract, frontend tests/build, release-verification entrypoint, and artifact scan are green in the current source snapshot.

## Real production-path qualification modules

Performance and differential qualification now live inside the application crate so they exercise production services and ports directly:

- `src-tauri/src/persistence/performance_tests.rs` uses real `PersistenceRuntime`, `RoutingService`, `RequestLogService`, `ChangeService`, and `RequestFinalizationService`; terminal persistence goes through actual request start and terminal finalization, not substitute finalization SQL.
- `src-tauri/src/persistence/differential_tests.rs` uses real crate modules and the `RequestLifecycleStore` contract.

Focused commands are:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::performance_tests -- --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests -- --nocapture --test-threads=1
```

The release qualification script invokes the performance module with `--release --locked`. Debug runs retain emitted samples but intentionally do not enforce release absolute timing limits; this avoids treating debug contention as a release timing failure and does not relax the release thresholds.

## Retained Task 15 execution evidence

The paired performance run was executed before this documentation-only update so its worktree provenance remains a truthful source snapshot. Do not edit production source without repeating the paired run.

```powershell
$e = 'D:\Dev\Build\relay-pool-persistence-v2-qualification'
New-Item -ItemType Directory -Force $e | Out-Null

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-persistence-v1-performance-baseline.ps1 `
  -OutputPath "$e\v0.3.1-baseline.json" `
  -CargoTargetDir 'D:\Dev\Build\relay-pool-persistence-v2-merge-check'

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\run-persistence-performance-qualification.ps1 `
  -BaselinePath "$e\v0.3.1-baseline.json" `
  -OutputPath "$e\paired-v0.3.1-v2.json" `
  -CargoTargetDir 'D:\Dev\Build\relay-pool-persistence-v2-merge-check'
```

The fixed V1 source commit is `54751559aed8f3f7c159e322bc7bbcc71d993204`; the released-fixture SHA-256 is `ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9`. The paired result must retain its V1/V2 raw JSON, machine details, power mode, background-load notes, and worktree provenance. It must establish the relative hot-read and startup gates without substituting an unrelated V2-only measurement.

## Exit decision

Task 15 is source-qualified: paired performance evidence, differential/fault coverage, SQLx offline metadata, contracts, frontend tests/build, and tracked artifact checks are green. Task 16 deletion implementation is locally validated but not committed. Task 17 remains an external signed-artifact and isolated-install qualification; no source-level check is a substitute for it.
