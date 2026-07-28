# 2026-07-22 Architecture Scale Deterministic Qualification

Date: 2026-07-28

## Scope

- Close Stage 7 Task 26 deterministic qualification for the architecture scale upgrade.
- Qualified source revision: `83defbc241d1b4d28ec930853d712adff4cfc671`.
- Worktree: `D:\Dev\Projects\relay-pool-desktop-architecture-scale-upgrade`.
- Branch: `codex/architecture-scale-upgrade`.
- Persistence V2 protected source and migrations were not modified.
- No desktop app launch, screenshot, or direct visual desktop inspection was used for this qualification.

## Manifest Reconciliation

- Updated `docs/superpowers/audits/persistence-v2-boundary-manifest.json` to match the current Stage 6/7 architecture graph.
- The manifest now has 493 sorted, deduplicated `allowed_edges`, all with `temporary: false`.
- Removed stale command and collector adapter allowances reported by the gate.
- Focused gate result: `cargo test --manifest-path src-tauri\Cargo.toml --test persistence_architecture -- --nocapture` passed with 35 tests.

## Deterministic Verification

- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile full`
  - exit code: 0
  - started at: `2026-07-28T00:35:49.4164641Z`
  - revision: `83defbc241d1b4d28ec930853d712adff4cfc671`
- The full profile covered:
  - architecture bypass fixtures
  - TypeScript boundaries
  - deterministic generated IPC bindings
  - command registry and Tauri security gates
  - production build entry and artifact policy gates
  - dependency lifecycle, advisory, license and source policy gates
  - ESLint, TypeScript check, frontend contract/unit/component tests and build
  - Rust architecture fixtures, fmt, clippy, check and test suites
  - tracked Persistence V2 artifact policy
  - deterministic frontend scale baseline generation

## Evidence

- Gitignored deterministic evidence summary:
  `output/architecture-scale/qualification/deterministic/task26-summary.json`
- Generated frontend scale baseline:
  `output/architecture-scale/baseline/frontend-report.json`
- Protected-path check:
  `src-tauri/src/persistence` and `src-tauri/migrations` had no working-tree changes.

## Known Warnings

- ESLint completed with existing unused-symbol warnings and no errors.
- Rust completed with existing dead-code, type-complexity, too-many-arguments and clippy advisory warnings and no failing gate.
- This Task 26 report is deterministic/local qualification only. Stage 7 Task 27 soak/live qualification and Task 28 release/locked build qualification remain separate gates.

## Result

Task 26 deterministic qualification passes for revision `83defbc241d1b4d28ec930853d712adff4cfc671`. Task 27 may start from this qualified baseline, but release readiness is not claimed until Task 27 and Task 28 pass.
