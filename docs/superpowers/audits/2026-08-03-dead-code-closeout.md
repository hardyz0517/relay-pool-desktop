# Rust Dead Code Closeout

Date: 2026-08-03
Status: implemented; non-production warning noise and signed-release environment validation remain tracked as follow-up/pre-release work

This closeout records the result of the dead-code reliability upgrade for `src-tauri` and the release verification path.

## Result

Production Rust dead-code debt is now gated instead of merely observed:

| Metric | Baseline | Current |
|---|---:|---:|
| normal `cargo check --lib` dead_code warning groups | 64 | 0 production diagnostics |
| `cargo check --all-targets` dead_code groups | 267 | still allowed to report test-target/source-included fixture noise |
| `cargo check --release --lib` dead_code groups | 64 | 0 production diagnostics |
| force-warn hidden diagnostics | 529 diagnostics / 525 unique | covered by source policy and audited contract expects |
| blanket `allow(dead_code)` in production policy | 69 baseline | 0 |
| local `allow(dead_code)` in production policy | 11 baseline | 0 |
| registered `expect(dead_code)` contracts | 2 baseline | 54 audited expects |
| test-support leakage | not gated | 0 |

The important distinction: the production policy is clean, but the whole repository is not literally warning-free. Remaining log noise is documented below and should be handled as separate cleanup, not by weakening the dead-code policy.

## What changed

- Removed stale Auto monitoring protocol selection and the unused `protocol_auto` adapter path.
- Removed stale routing/proxy/error bridge code and kept a single typed failure path.
- Wired `session_hash` and `previous_response_id` into the routing affinity path without bypassing eligibility, priority, capacity, or success-only binding rules.
- Wired revision-aware collector session persistence, then deleted the stale session update and precise credential invalidation chain that had no current caller.
- Deleted data maintenance admission/recovery wrappers that duplicated the persistence freeze boundary.
- Deleted portable migration no-operation-ID wrappers, allocation-style parser helpers, stale recovery evidence, and test-only safety probes from production builds.
- Converted IPC/DTO/registry dead-code allowances into audited `expect(dead_code)` contracts with owner and removal conditions.
- Removed production blanket/local `allow(dead_code)` use from the dead-code policy.
- Added `scripts/dead-code-inventory-policy.test.mjs` so intentional policy regressions are tested with fixtures.
- Updated `scripts/verify.ps1` so `verify:fast`, `verify:full`, and `verify:release` run the same dead-code policy checks.
- Updated `.github/workflows/release.yml` so release verification calls the shared release verifier through phase-specific scripts instead of maintaining a second release lint path or forwarding a literal `--` into PowerShell.
- Regenerated IPC bindings and command registry after contract changes.
- Updated architecture manifests for the new routing/query/persistence owner edges and removed stale allowlisted edges.

## Verification

Latest local verification:

| Command | Exit | Notes |
|---|---:|---|
| `node scripts/dead-code-inventory-policy.test.mjs` | 0 | policy fixtures pass |
| `node scripts/dead-code-inventory.mjs --mode ci --scope production` | 0 | production diagnostics 0; blanket/local allow 0; registered expects 54; test-support leakage 0 |
| `node scripts/release-verification-entrypoint.test.mjs` | 0 | release entrypoint behavior covered |
| `node scripts/updater-config.test.mjs` | 0 | release workflow uses the shared phase-specific verification scripts |
| `node scripts/updater-timeout-recovery.test.mjs` | 0 | updater recovery regression still runs through shared contract coverage |
| `pnpm.cmd generate:bindings --check` | 0 | generated bindings in sync |
| `pnpm.cmd test:contracts` | 0 | frontend/IPC contract checks pass |
| `node scripts/architecture/check-dependency-lifecycle.mjs` | 0 | dependency lifecycle gate accepts CI Rust 1.95.0 and local reference 1.97.1 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture` | 0 | 42 passed |
| `pnpm.cmd verify:fast` | 0 | passed; Windows linker stdout warnings only |
| `pnpm.cmd verify:full` with `CARGO_BUILD_JOBS=2` | 0 | passed; duration 333.73s on the latest closeout rerun |

One earlier `verify:full` run failed on Windows with `os error 1455` while Cargo was using high concurrency. The low-concurrency run with `CARGO_BUILD_JOBS=2` passed, so this is tracked as a local memory/page-file constraint rather than a code failure.

## Remaining warning/noise classes

These are intentionally not counted as production dead-code failures:

- `cargo check --all-targets` / test builds still emit Rust `dead_code` warnings from source-included or test-only surfaces. The main cleanup target is operational source-included fixtures; split them into narrower support modules or smaller stubs.
- `generate:bindings` and some test-target builds can still print Rust lib-test dead-code warnings because they compile source-included test surfaces.
- `cargo clippy --all-targets` still emits clippy warnings that are separate from the production dead-code policy.
- ESLint currently reports warnings, mainly unused variables in `src/lib/bridge/DemoBackend.ts` and a few UI files.
- `cargo-deny` reports warning-level dependency policy noise, including unmatched license allowances and duplicate dependency versions.
- Some contract tests emit Node `DEP0190` warnings.
- Vite production build emits a chunk-size warning.
- Windows MSVC linker can print stdout messages while creating `.lib` / `.exp` files; these are not Rust dead-code diagnostics.

## Release limitations

The local run did not fully execute a real signed release bundle. A complete `pnpm.cmd verify:release` may require:

- a matching Git tag because release version verification uses `--require-tag`;
- Tauri signing configuration via `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH`;
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when the signing key requires it.

The release workflow now uses the shared verification entrypoint, so production dead-code regressions should fail in the same way locally and in GitHub Actions.

Local phase-entry smoke evidence:

- `pnpm.cmd verify:release:prebundle` now reaches the release version contract and fails because `RELAY_POOL_RELEASE_TAG` is unset in this local checkout.
- `git describe --exact-match --tags HEAD` reports that no tag matches `706aed1ad89fb0b5cb2a97c305c846d5b6a30401`.
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PATH`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are unset locally, so the signed bundle phase is intentionally not runnable here.

## Reliability, maintainability, and extensibility impact

- Reliability: dead paths that looked like protection but were not actually reachable were either wired through real owners or removed. The remaining production contracts are explicitly marked and tested.
- Maintainability: duplicate bridges and blanket lint masks were removed, so future changes have fewer hidden owners and fewer “maybe this is still used” paths.
- Extensibility: new routing, migration, monitoring, or IPC contract code now has to declare an owner, behavior, and removal condition before it can stay in production builds.

## Follow-up recommendation

Treat the remaining work as warning-noise cleanup rather than production dead-code cleanup:

1. Split operational source-included integration fixtures into narrower test support.
2. Clean ESLint unused-variable warnings, especially demo backend and UI leftovers.
3. Review clippy/cargo-deny/Vite warnings under their own policies.
4. Run a signed `pnpm.cmd verify:release` in an environment with tag and signing secrets before an actual public release.
